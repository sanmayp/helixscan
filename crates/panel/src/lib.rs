use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Seed {
    pub section: String,
    pub trait_group: String,
    pub expected_gene: String,
    pub rsid: String,
    pub keyword_regex: String,
    pub note: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Phenotype {
    pub trait_text: String,
    pub risk_allele: Option<String>,
    pub p_value: Option<String>,
    pub pmid: Option<String>,
    pub source: Option<String>,
    pub odds_ratio: Option<String>,
    pub beta_coefficient: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Variant {
    pub rsid: String,
    pub gene: String,
    pub trait_group: String,
    pub section: String,
    pub note: String,
    #[serde(default)]
    pub allele_offset: usize,
    #[serde(default)]
    pub raw_ref: String,
    #[serde(default)]
    pub raw_alts: Vec<String>,
    pub chr: String,
    pub pos: u64,
    pub end: u64,
    pub reference: String,
    #[serde(rename = "ref")]
    pub ref_allele: String,
    pub alts: Vec<String>,
    pub flank_left: String,
    pub flank_right: String,
    pub phenotypes: Vec<Phenotype>,
    /// Unique allele supported by GWAS evidence or an explicit curated row.
    #[serde(default)]
    pub effect_allele: Option<String>,
    #[serde(default)]
    pub effect_allele_status: EffectAlleleStatus,
    #[serde(default)]
    pub curated_effect: Option<CuratedEffect>,
    #[serde(default)]
    pub effect_findings: Vec<Phenotype>,
    pub clinical_significance: Vec<String>,
    pub frequencies: BTreeMap<String, BTreeMap<String, f64>>,
    pub fetched_at: String,
    pub ensembl_release: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Haplotype {
    pub name: String,
    pub rs429358: String,
    pub rs7412: String,
}

/// A small, versioned, manually reviewed override. It is embedded from
/// `data/curated_effects.toml` and serialized into the panel and each genotype
/// call so report provenance does not depend on another file being present.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CuratedEffect {
    pub gene: String,
    pub rsid: String,
    pub effect_allele: String,
    pub effect_direction: String,
    pub plain_meaning: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CuratedEffects {
    pub version: u32,
    #[serde(rename = "effects", default)]
    pub rows: Vec<CuratedEffect>,
}

pub fn curated_effects() -> Result<Vec<CuratedEffect>> {
    let table: CuratedEffects = toml::from_str(include_str!("../../../data/curated_effects.toml"))
        .context("parse embedded data/curated_effects.toml")?;
    if table.version != 1 {
        bail!("unsupported curated-effect version {}", table.version);
    }
    let mut seen = std::collections::BTreeSet::new();
    for row in &table.rows {
        if row.gene.is_empty()
            || row.rsid.is_empty()
            || row.effect_direction.is_empty()
            || row.plain_meaning.is_empty()
            || row.source.is_empty()
        {
            bail!("curated-effect rows must have all metadata fields");
        }
        if !seen.insert(row.rsid.clone()) {
            bail!("duplicate curated effect for {}", row.rsid);
        }
        if !row.effect_allele.is_empty() && row.effect_allele.chars().count() > 64 {
            bail!("curated effect allele is too long for {}", row.rsid);
        }
    }
    Ok(table.rows)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectAlleleStatus {
    Supported,
    Curated,
    #[default]
    Unclear,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct EffectAudit {
    pub old_allele: Option<String>,
    pub effect_allele: Option<String>,
    pub status: EffectAlleleStatus,
    pub own_trait_findings: usize,
    pub qualifying_findings: usize,
    pub decreasing_evidence: Vec<String>,
    pub effect_findings: Vec<Phenotype>,
}

/// Only exact, case-insensitive trait matches with OR > 1 or a beta that
/// explicitly says "unit increase" may establish an effect allele.
/// Decreasing effects and directionless labels never do. Different alleles,
/// no qualifying findings, or an allele absent from the panel mean unclear.
pub fn effect_audit(v: &Variant) -> EffectAudit {
    let old_allele = v.phenotypes.iter().find_map(|p| p.risk_allele.clone());
    let own = v
        .phenotypes
        .iter()
        .filter(|p| p.trait_text.eq_ignore_ascii_case(&v.trait_group))
        .collect::<Vec<_>>();
    let qualifying = own
        .iter()
        .filter(|p| {
            p.risk_allele.is_some()
                && (p
                    .odds_ratio
                    .as_deref()
                    .and_then(|x| x.trim().parse::<f64>().ok())
                    .is_some_and(|x| x.is_finite() && x > 1.0)
                    || p.beta_coefficient
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|x| x.to_ascii_lowercase().contains("unit increase")))
        })
        .map(|p| (*p).clone())
        .collect::<Vec<_>>();
    let alleles = qualifying
        .iter()
        .filter_map(|p| p.risk_allele.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let supported = alleles.len() == 1
        && alleles
            .iter()
            .next()
            .is_some_and(|a| a == &v.ref_allele || v.alts.contains(a));
    EffectAudit {
        old_allele,
        effect_allele: if supported {
            alleles.into_iter().next()
        } else {
            None
        },
        status: if supported {
            EffectAlleleStatus::Supported
        } else {
            EffectAlleleStatus::Unclear
        },
        own_trait_findings: own.len(),
        qualifying_findings: qualifying.len(),
        decreasing_evidence: own
            .into_iter()
            .filter(|p| {
                p.beta_coefficient
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|x| x.to_ascii_lowercase().contains("unit decrease"))
            })
            .map(|p| {
                format!(
                    "{} beta {}",
                    p.risk_allele.as_deref().unwrap_or("(no allele)"),
                    p.beta_coefficient.as_deref().unwrap_or("")
                )
            })
            .take(4)
            .collect(),
        effect_findings: if supported { qualifying } else { vec![] },
    }
}

/// Rebuild GWAS-only results, then override only the exact curated rows.
/// This function is intentionally total: an invalid table cannot silently
/// select an allele or leave a stale override on a rebuilt panel.
pub fn audit_panel(panel: &mut Panel) {
    let all: std::collections::BTreeSet<_> =
        panel.variants.iter().map(|v| v.rsid.clone()).collect();
    // Builders and report subsets can legitimately omit part of the full
    // panel. The CLI still audits the full panel strictly below.
    let complete = curated_effects()
        .expect("parse embedded curated effects")
        .iter()
        .all(|row| all.contains(&row.rsid));
    audit_panel_inner(panel, complete).expect("invalid embedded curated effects");
}

pub fn audit_panel_checked(panel: &mut Panel) -> Result<()> {
    audit_panel_inner(panel, true)
}

fn audit_panel_inner(panel: &mut Panel, require_all: bool) -> Result<()> {
    let curated = curated_effects()?;
    let mut table = curated
        .into_iter()
        .map(|row| (row.rsid.clone(), row))
        .collect::<BTreeMap<_, _>>();
    for v in &mut panel.variants {
        let audit = effect_audit(v);
        v.effect_allele = audit.effect_allele;
        v.effect_allele_status = audit.status;
        v.effect_findings = audit.effect_findings;
        v.curated_effect = None;
        let matching = table.remove(&v.rsid);
        if let Some(row) = matching {
            if row.gene != v.gene {
                bail!(
                    "curated {} gene {} does not match panel gene {}",
                    row.rsid,
                    row.gene,
                    v.gene
                );
            }
            if row.effect_allele != v.ref_allele && !v.alts.contains(&row.effect_allele) {
                bail!(
                    "curated {} allele {:?} is not a panel REF/ALT allele",
                    row.rsid,
                    row.effect_allele
                );
            }
            v.effect_allele = Some(row.effect_allele.clone());
            v.effect_allele_status = EffectAlleleStatus::Curated;
            v.curated_effect = Some(row);
        }
    }
    if require_all && !table.is_empty() {
        bail!(
            "curated rsIDs not found exactly once in panel: {}",
            table.into_keys().collect::<Vec<_>>().join(", ")
        );
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Panel {
    pub schema_version: u32,
    pub reference: String,
    pub ensembl_release: u32,
    pub fetched_at: String,
    #[serde(default)]
    pub pgx_rules: String,
    #[serde(default)]
    pub searched_traits: Vec<SearchedTrait>,
    #[serde(default)]
    pub marker_regions: Vec<MarkerRegion>,
    pub variants: Vec<Variant>,
    pub apoe_haplotypes: Vec<Haplotype>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchedTrait {
    pub section: String,
    pub trait_group: String,
    pub term: String,
    pub markers: Vec<String>,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerRegion {
    pub name: String,
    pub chr: String,
    pub start: u64,
    pub end: u64,
    pub sequence: String,
}
pub mod effect_report;
#[cfg(test)]
mod tests;

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

pub fn parse_seed(path: &Path) -> Result<Vec<Seed>> {
    let text = fs::read_to_string(path)?;
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("section\t"))
        .map(|line| {
            let p: Vec<_> = line.split('\t').collect();
            assert!(p.len() >= 5, "seed needs six columns");
            Seed {
                section: p[0].into(),
                trait_group: p[1].into(),
                expected_gene: p[2].into(),
                rsid: p[3].into(),
                keyword_regex: p[4].into(),
                note: p.get(5).unwrap_or(&"").to_string(),
            }
        })
        .collect())
}
pub trait Rest {
    fn json(&self, path: &str) -> Result<Value>;
}
#[derive(Clone, Default)]
pub struct Ensembl {
    pub release: u32,
    pub delay: Duration,
}
impl Ensembl {
    pub fn get(&self, path: &str) -> Result<ureq::http::Response<ureq::Body>> {
        let mut last = None;
        for attempt in 0..3 {
            if !self.delay.is_zero() {
                thread::sleep(self.delay);
            }
            match ureq::get(&format!("https://rest.ensembl.org/{path}"))
                .header("Content-Type", "application/json")
                .header("User-Agent", "helixscan/0.1")
                .config()
                .timeout_global(Some(Duration::from_secs(30)))
                .build()
                .call()
            {
                Ok(r) => return Ok(r),
                Err(e) => {
                    last = Some(e);
                    if attempt < 2 {
                        thread::sleep(Duration::from_secs(1 << attempt));
                    }
                }
            }
        }
        Err(anyhow!(
            "Ensembl request failed after 3 retries: {}",
            last.unwrap()
        ))
    }
}
impl Rest for Ensembl {
    fn json(&self, path: &str) -> Result<Value> {
        let cache_dir = Path::new(".panel-cache");
        let name = path
            .replace('.', "-")
            .replace(['/', '?', ':', '&', '=', '*'], "_");
        let cache = cache_dir.join(format!("{name}.json"));
        if let Ok(bytes) = fs::read(&cache)
            && let Ok(value) = serde_json::from_slice(&bytes)
        {
            return Ok(value);
        }
        let value: Value = self.get(path)?.into_body().read_json()?;
        fs::create_dir_all(cache_dir)?;
        fs::write(&cache, serde_json::to_vec(&value)?)?;
        Ok(value)
    }
}
pub fn normalize_alleles(
    reference: &str,
    alternates: &[String],
) -> Result<(usize, String, Vec<String>)> {
    let alleles: Vec<String> = std::iter::once(reference.to_string())
        .chain(
            alternates
                .iter()
                .map(|a| if a == "-" { "".into() } else { a.clone() }),
        )
        .collect();
    let mut prefix = 0;
    while alleles
        .iter()
        .all(|a| prefix < a.len() && a.as_bytes()[prefix] == alleles[0].as_bytes()[prefix])
    {
        prefix += 1;
    }
    // Prefix-only normalization is deterministic even in tandem repeats.
    // allele_context verifies the REF event against the genomic sequence.
    let trim = |a: &str| a[prefix..].to_string();
    Ok((
        prefix,
        trim(reference),
        alleles[1..].iter().map(|a| trim(a)).collect(),
    ))
}

pub fn allele_context(v: &Variant, allele: &str) -> (Vec<u8>, usize) {
    let mut seq = v.flank_left.clone().into_bytes();
    seq.extend_from_slice(v.reference.as_bytes());
    seq.extend_from_slice(v.flank_right.as_bytes());
    let pos = v.flank_left.len() + v.allele_offset;
    assert!(
        pos + v.ref_allele.len() <= seq.len(),
        "allele offset outside context"
    );
    assert_eq!(
        &seq[pos..pos + v.ref_allele.len()],
        v.ref_allele.as_bytes(),
        "normalized REF does not match genomic context"
    );
    seq.splice(
        pos..pos + v.ref_allele.len(),
        allele.as_bytes().iter().copied(),
    );
    (seq, pos)
}

pub struct PanelBuilder<C: Rest> {
    pub client: C,
    pub drops: Vec<String>,
}
impl<C: Rest> PanelBuilder<C> {
    pub fn new(client: C) -> Self {
        Self {
            client,
            drops: Vec::new(),
        }
    }
    pub fn build(&mut self, seed_path: &Path, limit: usize) -> Result<Panel> {
        let seeds = parse_seed(seed_path)?;
        let now = chrono::Utc::now().to_rfc3339();
        let mut variants = Vec::new();
        let release = self
            .client
            .json("info/data?")?
            .get("releases")
            .and_then(Value::as_array)
            .and_then(|x| x[0].as_u64())
            .unwrap_or(0) as u32;
        let seeds: Vec<_> = seeds
            .into_iter()
            .take(if limit == 0 { usize::MAX } else { limit })
            .collect();
        let total = seeds.len();
        for (index, seed) in seeds.into_iter().enumerate() {
            eprintln!("[{}/{}] {}", index + 1, total, seed.rsid);
            match self.fetch(&seed, release, &now) {
                Ok(Some(v)) => variants.push(v),
                Ok(None) => {}
                Err(e) => self.drops.push(format!(
                    "{}\trequest failed after 3 retries: {e:#}",
                    seed.rsid
                )),
            }
        }
        let mut searched_traits = Vec::new();
        let terms = seed_path
            .parent()
            .unwrap_or(Path::new("data"))
            .join("panel.terms.tsv");
        if let Ok(text) = fs::read_to_string(terms) {
            for line in text.lines().skip(1) {
                let fields: Vec<_> = line.split('\t').collect();
                if fields.len() != 4 {
                    continue;
                }
                let (section, trait_group, term, maximum) = (
                    fields[0],
                    fields[1],
                    fields[2],
                    fields[3].parse::<usize>().unwrap_or(0),
                );
                match self.search_term(section, trait_group, term, maximum, release, &now) {
                    Ok((markers, found_variants)) => {
                        variants.extend(found_variants);
                        let message = if searched_traits.last().is_some_and(|_| false) {
                            unreachable!()
                        } else if markers.is_empty() {
                            let mut m = "No well-proven genetic markers are known for this yet."
                                .to_string();
                            if term == "amblyopia" || term == "strabismus" {
                                m.push_str(" Children's eye checks matter most: lazy eye is treated best when found early.");
                            }
                            m
                        } else {
                            "Variants found by Ensembl term search; details are stored with each marker.".into()
                        };
                        searched_traits.push(SearchedTrait {
                            section: section.into(),
                            trait_group: trait_group.into(),
                            term: term.into(),
                            markers,
                            message,
                        });
                    }
                    Err(e) => self
                        .drops
                        .push(format!("term {term}\tsearch failed: {e:#}")),
                }
            }
        }
        if variants.is_empty() {
            bail!("no verified variants could be built");
        }
        let marker_regions = self.fetch_marker_regions();
        let mut panel = Panel {
            schema_version: 1,
            reference: "GRCh38".into(),
            ensembl_release: release,
            fetched_at: now,
            pgx_rules: String::new(),
            searched_traits,
            marker_regions,
            variants,
            apoe_haplotypes: vec![
                Haplotype {
                    name: "e2".into(),
                    rs429358: "T".into(),
                    rs7412: "T".into(),
                },
                Haplotype {
                    name: "e3".into(),
                    rs429358: "T".into(),
                    rs7412: "C".into(),
                },
                Haplotype {
                    name: "e4".into(),
                    rs429358: "C".into(),
                    rs7412: "C".into(),
                },
            ],
        };
        audit_panel(&mut panel);
        Ok(panel)
    }
    fn fetch_marker_regions(&self) -> Vec<MarkerRegion> {
        let names = [
            ("SRY", "Y"),
            ("UTY", "Y"),
            ("RPS4X", "X"),
            ("KDM6A", "X"),
            ("ACTB", "7"),
        ];
        let mut regions = Vec::new();
        for (name, expected_chr) in names {
            match self
                .client
                .json(&format!("lookup/symbol/homo_sapiens/{name}?expand=0"))
                .and_then(|g| {
                    let chr = text(&g, "seq_region_name")
                        .context("chromosome")?
                        .to_string();
                    let start = g.get("start").and_then(Value::as_u64).context("start")?;
                    let end = g.get("end").and_then(Value::as_u64).context("end")?;
                    if chr != expected_chr {
                        bail!("unexpected chromosome for {name}");
                    }
                    // A bounded gene-region sequence is sufficient for this
                    // rough control; avoid multi-megabyte UTY/ACTB downloads.
                    let seq_end = end.min(start + 19_999);
                    let sequence = self
                        .client
                        .json(&format!("sequence/region/human/{chr}:{start}..{seq_end}:1"))?;
                    Ok(MarkerRegion {
                        name: name.into(),
                        chr,
                        start,
                        end,
                        sequence: text(&sequence, "seq")
                            .context("sequence")?
                            .to_ascii_uppercase(),
                    })
                }) {
                Ok(region) => regions.push(region),
                Err(e) => eprintln!("marker {name}: {e:#}"),
            }
        }
        regions
    }
    fn search_term(
        &mut self,
        section: &str,
        trait_group: &str,
        term: &str,
        maximum: usize,
        release: u32,
        now: &str,
    ) -> Result<(Vec<String>, Vec<Variant>)> {
        let response = self.client.json(&format!(
            "phenotype/term/homo_sapiens/{}?include_children=1",
            term.replace(' ', "%20")
        ))?;
        let mut candidates = Vec::new();
        for item in response.as_array().into_iter().flatten() {
            let Some(rsid) = text(item, "Variation") else {
                continue;
            };
            if !rsid.starts_with('r') || !rsid[2..].chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let source = text(item, "source").unwrap_or("");
            let attrs = item.get("attributes");
            let pvalue = attrs
                .and_then(|a| a.get("p_value"))
                .and_then(|x| x.as_str())
                .and_then(|x| x.parse::<f64>().ok());
            let significance = attrs
                .and_then(|a| a.get("clinical_significance"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if !(source == "NHGRI-EBI GWAS catalog" && pvalue.is_some_and(|p| p <= 5e-8)
                || source.contains("ClinVar")
                    && (significance.contains("pathogenic")
                        || significance.contains("risk factor")))
            {
                continue;
            }
            candidates.push((
                pvalue.unwrap_or(0.0),
                rsid.to_string(),
                attrs
                    .and_then(|a| a.get("external_reference"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ));
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        candidates.dedup_by(|a, b| a.1 == b.1);
        let mut markers = Vec::new();
        let mut found_variants = Vec::new();
        for (_, rsid, pmid) in candidates.into_iter().take(maximum) {
            let seed = Seed {
                section: section.into(),
                trait_group: trait_group.into(),
                expected_gene: String::new(),
                rsid: rsid.clone(),
                keyword_regex: regex::escape(term),
                note: format!("found by term search; {}", pmid),
            };
            match self.fetch(&seed, release, now) {
                Ok(Some(mut variant)) => {
                    variant.note = format!("found by term search; {}; PMID {}", term, pmid);
                    markers.push(rsid);
                    found_variants.push(variant);
                }
                Ok(None) => {}
                Err(e) => self
                    .drops
                    .push(format!("{rsid}\tterm validation failed: {e:#}")),
            }
        }
        Ok((markers, found_variants))
    }
    pub fn fetch_variant(&mut self, s: &Seed, release: u32, now: &str) -> Result<Option<Variant>> {
        self.fetch(s, release, now)
    }
    fn fetch(&mut self, s: &Seed, release: u32, now: &str) -> Result<Option<Variant>> {
        let v = self
            .client
            .json(&format!("variation/human/{}?phenotypes=1", s.rsid))?;
        let m = v
            .get("mappings")
            .and_then(Value::as_array)
            .and_then(|m| {
                m.iter().find(|m| {
                    m.get("assembly_name").and_then(Value::as_str) == Some("GRCh38")
                        && m.get("coord_system").and_then(Value::as_str) == Some("chromosome")
                })
            })
            .context("no GRCh38 chromosome mapping")?;
        let chr = text(m, "seq_region_name")
            .context("no chromosome")?
            .to_string();
        let mut pos = m.get("start").and_then(Value::as_u64).context("no start")?;
        let end = m.get("end").and_then(Value::as_u64).context("no end")?;
        let alleles: Vec<_> = text(m, "allele_string")
            .context("no alleles")?
            .split('/')
            .map(str::to_string)
            .collect();
        if alleles.len() < 2 {
            self.drops.push(format!("{}\tno alternate allele", s.rsid));
            return Ok(None);
        }
        if alleles
            .iter()
            .any(|a| a.len() > 21 || (!a.chars().all(|c| "ACGTN-".contains(c))))
        {
            self.drops.push(format!(
                "{}\tunsupported alleles (>20 bp or non-IUPAC)",
                s.rsid
            ));
            return Ok(None);
        }
        let genes = self.client.json(&format!(
            "overlap/region/human/{chr}:{}-{}?feature=gene",
            pos.saturating_sub(50_000).max(1),
            end + 50_000
        ))?;
        let mut found = false;
        let mut distance = 0;
        for g in genes.as_array().into_iter().flatten() {
            if text(g, "external_name")
                .unwrap_or("")
                .eq_ignore_ascii_case(&s.expected_gene)
            {
                let gs = g.get("start").and_then(Value::as_u64).unwrap_or(0);
                let ge = g.get("end").and_then(Value::as_u64).unwrap_or(0);
                distance = if pos < gs {
                    gs - end
                } else if end > ge {
                    pos - ge
                } else {
                    0
                };
                found = true;
            }
        }
        if !found && !s.expected_gene.is_empty() {
            self.drops.push(format!(
                "{}\texpected gene {} absent within 50 kb",
                s.rsid, s.expected_gene
            ));
            return Ok(None);
        }
        let sig: Vec<String> = v
            .get("clinical_significance")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let re = Regex::new(&format!("(?i){}", s.keyword_regex))?;
        let all_p = v
            .get("phenotypes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let phenotypes: Vec<_> = all_p
            .iter()
            .filter(|p| {
                text(p, "trait").is_some_and(|x| re.is_match(x))
                    || p.get("genes")
                        .and_then(Value::as_array)
                        .is_some_and(|g| g.iter().filter_map(Value::as_str).any(|x| re.is_match(x)))
            })
            .map(|p| Phenotype {
                trait_text: text(p, "trait").unwrap_or("").into(),
                risk_allele: text(p, "risk_allele").map(str::to_string),
                p_value: text(p, "pvalue").map(str::to_string),
                pmid: text(p, "study")
                    .filter(|s| s.starts_with("PMID:"))
                    .map(str::to_string),
                source: text(p, "source").map(str::to_string),
                odds_ratio: p.get("odds_ratio").and_then(|o| {
                    o.as_str()
                        .map(str::to_string)
                        .or_else(|| o.as_f64().map(|x| x.to_string()))
                }),
                beta_coefficient: text(p, "beta_coefficient").map(str::to_string),
            })
            .collect();
        let keyword_pass = s.expected_gene.is_empty()
            || sig.iter().any(|x| re.is_match(x))
            || !phenotypes.is_empty()
            || re.is_match(&s.expected_gene);
        if !keyword_pass {
            self.drops.push(format!(
                "{}\tkeyword '{}' absent from clinical significance and phenotype/gene text",
                s.rsid, s.keyword_regex
            ));
            return Ok(None);
        }
        let insertion = end < pos;
        if insertion {
            pos = end + 1;
        }
        let seq: Value = self.client.json(&format!(
            "sequence/region/human/{chr}:{}..{}:1",
            pos.saturating_sub(60).max(1),
            end + 60
        ))?;
        let sequence = text(&seq, "seq")
            .context("no reference sequence")?
            .to_ascii_uppercase();
        let l = 60.min(pos.saturating_sub(1) as usize);
        if sequence.len() != l + (end - pos + 1) as usize + 60 {
            bail!("unexpected flank sequence length for {}", s.rsid);
        }
        let mut reference = sequence[l..l + (end + 1 - pos) as usize].to_string();
        if alleles[0] != reference && alleles[0] != "-" {
            self.drops.push(format!(
                "{}\tEnsembl reference {} does not match GRCh38 reference {}",
                s.rsid, alleles[0], reference
            ));
            return Ok(None);
        }
        if alleles[0] == "-" {
            reference = "-".into();
        }
        let flank_left = sequence[..l].to_string();
        let flank_right = sequence[l + (end + 1 - pos) as usize..].to_string();
        let anchored_alts = if insertion {
            alleles[1..]
                .iter()
                .map(|a| format!("{reference}{}", a.replace('-', "")))
                .collect::<Vec<_>>()
        } else {
            alleles[1..].to_vec()
        };
        let (allele_offset, ref_allele, alts) = normalize_alleles(&reference, &anchored_alts)?;
        let pops = self
            .client
            .json(&format!("variation/human/{}?pops=1", s.rsid))?;
        let mut frequencies: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
        for p in pops
            .get("populations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let pop = text(p, "population").unwrap_or("");
            let allele = text(p, "allele").unwrap_or("");
            if let Some(group) = pop.strip_prefix("1000GENOMES:phase_3:")
                && ["AFR", "AMR", "EAS", "EUR", "SAS"].contains(&group)
            {
                frequencies.entry(group.into()).or_default().insert(
                    allele.into(),
                    p.get("frequency").and_then(Value::as_f64).unwrap_or(0.0),
                );
            }
        }
        if distance > 0 {
            eprintln!(
                "{}: matched gene {} at distance {} bp",
                s.rsid, s.expected_gene, distance
            );
        }
        Ok(Some(Variant {
            rsid: s.rsid.clone(),
            gene: if s.expected_gene.is_empty() {
                genes
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|g| text(g, "external_name"))
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("/")
            } else {
                s.expected_gene.clone()
            },
            trait_group: s.trait_group.clone(),
            section: s.section.clone(),
            note: s.note.clone(),
            chr,
            allele_offset,
            raw_ref: alleles[0].clone(),
            raw_alts: alleles[1..].to_vec(),
            ref_allele,
            alts,
            pos,
            end,
            reference,
            flank_left,
            flank_right,
            phenotypes,
            effect_allele: None,
            effect_allele_status: EffectAlleleStatus::Unclear,
            effect_findings: vec![],
            curated_effect: None,
            clinical_significance: sig,
            frequencies,
            fetched_at: now.into(),
            ensembl_release: release,
        }))
    }
}
