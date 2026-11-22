use crate::{Panel, genotype::GenotypeCall, report::PgxReport};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RuleData {
    version: u32,
    genes: Vec<GeneRule>,
}
#[derive(Debug, Deserialize)]
struct GeneRule {
    gene: String,
    markers: Vec<String>,
    consult: String,
    rules: Vec<Condition>,
}
#[derive(Debug, Deserialize)]
struct Condition {
    when: String,
    text: String,
}
#[derive(Clone, Copy)]
enum Operator {
    Eq,
    Gt,
    Ge,
}
fn condition(text: &str) -> Option<(&str, Operator, u8)> {
    let (expr, op, value) = if let Some((expr, value)) = text.split_once(">=") {
        (expr, Operator::Ge, value)
    } else if let Some((expr, value)) = text.split_once('>') {
        (expr, Operator::Gt, value)
    } else {
        let (expr, value) = text.split_once('=')?;
        (expr, Operator::Eq, value)
    };
    Some((expr.trim(), op, value.trim().parse().ok()?))
}
fn compare(value: u8, op: Operator, expected: u8) -> bool {
    match op {
        Operator::Eq => value == expected,
        Operator::Gt => value > expected,
        Operator::Ge => value >= expected,
    }
}
/// Evaluate allele-count rules from the panel's versioned TOML data.
pub fn pgx_interpretations(panel: &Panel, calls: &[GenotypeCall]) -> Vec<PgxReport> {
    interpret(&panel.pgx_rules, panel, calls).unwrap_or_else(|error| {
        vec![PgxReport {
            gene: "Rule-data error".into(),
            phenotype: format!("Pharmacogenetic rules could not be evaluated: {error}"),
            consult:
                "Show this to your doctor or pharmacist; do not change any medicine on your own."
                    .into(),
        }]
    })
}
fn copies(panel: &Panel, calls: &[GenotypeCall], id: &str, allele: &str) -> Result<u8, String> {
    let i = panel
        .variants
        .iter()
        .position(|v| v.rsid == id)
        .ok_or("missing marker")?;
    if !calls[i].called {
        return Err("uncalled marker".into());
    }
    let copies = calls[i]
        .genotype
        .split('/')
        .filter(|a| *a == allele)
        .count();
    Ok(copies.min(2) as u8)
}
fn interpret(data: &str, panel: &Panel, calls: &[GenotypeCall]) -> Result<Vec<PgxReport>, String> {
    let rule_data: RuleData = toml::from_str(data).map_err(|e| e.to_string())?;
    if rule_data.version != 1 {
        return Err(format!("unsupported rule version {}", rule_data.version));
    }
    let mut output = Vec::new();
    for gene in rule_data.genes {
        let missing = gene
            .markers
            .iter()
            .filter(|id| {
                panel
                    .variants
                    .iter()
                    .zip(calls)
                    .any(|(variant, call)| variant.rsid == **id && !call.called)
            })
            .count();
        let text = if missing > 0 {
            format!(
                "Insufficient coverage at {missing} of {} checked markers; no phenotype call.",
                gene.markers.len()
            )
        } else {
            let value = |name: &str| -> Result<u8, String> {
                let (id, allele) = name.split_once(':').ok_or("value must be rsID:allele")?;
                copies(panel, calls, id, allele)
            };
            let mut selected = None;
            for rule in &gene.rules {
                if rule.when == "default" {
                    continue;
                }
                let matched = if rule.when == "any_alt" {
                    gene.markers
                        .iter()
                        .map(|id| {
                            let index = panel
                                .variants
                                .iter()
                                .position(|v| v.rsid == *id)
                                .ok_or("missing marker")?;
                            let alt = panel.variants[index]
                                .alts
                                .first()
                                .ok_or("missing alternate")?;
                            copies(panel, calls, id, alt)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                        .iter()
                        .sum::<u8>()
                        > 0
                } else {
                    let mut conjunction = true;
                    for clause in rule.when.split(" and ") {
                        let (name, op, expected) = condition(clause).ok_or("invalid condition")?;
                        conjunction &= compare(value(name)?, op, expected);
                    }
                    conjunction
                };
                if matched {
                    selected = Some(rule.text.clone());
                    break;
                }
            }
            selected
                .or_else(|| {
                    gene.rules
                        .iter()
                        .find(|r| r.when == "default")
                        .map(|r| r.text.clone())
                })
                .ok_or("no matching rule or default")?
        };
        output.push(PgxReport {
            gene: gene.gene,
            phenotype: text,
            consult: gene.consult,
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{Operator, compare, condition};

    #[test]
    fn comparisons_are_distinct_and_parse_allele_expressions() {
        let (name, op, value) = condition("rs12248560:T>=2").unwrap();
        assert_eq!(name, "rs12248560:T");
        assert!(matches!(op, Operator::Ge));
        assert_eq!(value, 2);
        assert!(compare(2, op, value));
        assert!(!compare(1, op, value));
        let (_, op, _) = condition("rs4244285:A>0").unwrap();
        assert!(matches!(op, Operator::Gt));
        assert!(!compare(0, op, 0));
        assert!(compare(1, op, 0));
        let (_, op, _) = condition("rs4244285:A=0").unwrap();
        assert!(matches!(op, Operator::Eq));
        assert!(condition("nonsense").is_none());
    }
}
