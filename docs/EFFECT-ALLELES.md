# Effect-allele audit

Source snapshot: `data/panel.json`, Ensembl release 116, plus H0c `data/curated_effects.toml` version 1. This audit covers **all 77 variants**. Original Ensembl `risk_allele` values and read-derived genotypes have not been overwritten.

## Statuses and dosage

- `supported`: unique allele from the conservative GWAS rule below.
- `curated`: an exact rsID/gene/REF-or-ALT match from the small, versioned hand-curated table. Curated rows replace either `unclear` or the generic GWAS result. Each genotype call repeats the row as `curated_effect` and has `effect_allele_status = "curated"`, so JSON carries its direction, plain meaning and source.
- `unclear`: no selected effect allele; `dosage = null`. Every non-curated variant keeps its original GWAS-only supported/unclear disposition. A curated row does not turn nearby unlisted rsIDs or other ALT alleles into supported effects.

Dosage is a count of the specifically named effect allele, **not** a disease probability, diagnosis or medication dose. A beneficial or functional allele can be counted just as a susceptibility allele can. For haplotypes and medication interpretation, read the row meaning plus the APOE/PGx results, not dosage alone.

## Conservative GWAS rule

Only findings whose `trait_text` equals the card's `trait_group` (case-insensitive) are eligible. Of those, only an odds ratio strictly greater than 1 or a beta containing `unit increase` carries an increasing-risk direction. Decreasing effects, other beta units, absent effect measures, ClinVar labels without an effect measure, gene names and allele frequencies do **not** select an effect allele. Multiple eligible alleles, a missing eligible allele, or an allele absent from the panel make it `unclear`.

No eligible odds ratios are recorded in this Ensembl snapshot; retained alleles therefore come from own-trait beta `unit increase` findings. Trait labels such as `Heart and cholesterol` are not literal Ensembl traits, so no finding qualifies for those cards under this rule. The H0c table is a deliberately separate provenance layer and does not rewrite those findings.

Evidence remains available for inspection but does not itself imply direction or clinical validity. This is not an estimate of personal risk.

## Curated overrides

All alleles below are GRCh38 forward-strand alleles as represented by the panel, not necessarily bases on the gene transcript. Source identifiers refer to ClinVar, PharmGKB/CPIC guidance, or the key paper. ClinVar records can be opened as `https://www.ncbi.nlm.nih.gov/clinvar/variation/<numeric-VCV-id>/`; PMIDs as `https://pubmed.ncbi.nlm.nih.gov/<PMID>/`. Gene/drug guidance is indexed at <https://www.clinpgx.org/cpic/pairs> and <https://www.pharmgkb.org/>.

| Variant | Gene | Effect allele | Direction | Plain meaning | Source |
|---|---|---|---|---|---|
| rs7903146 | TCF7L2 | T | higher type 2 diabetes risk | T is the established TCF7L2 type 2 diabetes susceptibility allele; this common association is not deterministic. | PMID:17463248 (Scott et al., Science 2007; rs7903146 T2D association) |
| rs9939609 | FTO | A | higher type 2 diabetes risk mediated partly through BMI | A is the established FTO obesity- and BMI-associated allele; it is associated with type 2 diabetes partly through body weight. | PMID:17434869 (Frayling et al., Science 2007) |
| rs429358 | APOE | C | APOE e4-defining allele, only with rs7412 C | C together with rs7412 C defines APOE e4, an established Alzheimer's susceptibility haplotype; neither SNP is interpreted alone as an APOE genotype. | ClinVar VCV000017864 (isolated APOE c.388T>C/rs429358 C allele); PharmGKB APOE allele definitions |
| rs7412 | APOE | T | APOE e2-defining allele, only with rs429358 T; not an e4 allele | T together with rs429358 T defines APOE e2. This allele does not define APOE e4; the two-site haplotype report is authoritative. | ClinVar VCV000017848 (isolated APOE c.526C>T/rs7412 T allele); PMID:7920638 (Corder et al., Nature Genetics 1994, APOE e2) |
| rs1800562 | HFE | A | HFE-related hereditary hemochromatosis-associated C282Y variant | A encodes HFE C282Y. Clinical hemochromatosis has incomplete penetrance; genotype alone does not establish iron overload. | ClinVar VCV000000009 (HFE c.845G>A, p.Cys282Tyr) |
| rs1799945 | HFE | G | HFE-related H63D variant; lower penetrance than C282Y, including in compound genotypes | G encodes HFE H63D. Its clinical significance depends on the other HFE allele and other findings; genotype alone is not iron overload. | ClinVar VCV000000010 (HFE c.187C>G, p.His63Asp; record includes conflicting classifications) |
| rs6025 | F5 | T | factor V Leiden-associated increased thrombosis risk | GRCh38 forward-strand T corresponds to F5 c.1601G>A (reverse-strand transcript; legacy c.1691G>A numbering), factor V Leiden. The other panel alternate, A, is not Leiden. | ClinVar VCV000000642 (rs6025; F5 c.1601G>A, p.Arg534Gln); PMID:8164741 (Bertina et al., Nature 1994) |
| rs1799963 | F2 | A | higher thrombosis risk | A is the F2 3-prime-UTR G20210A thrombosis-associated allele; carrying it does not mean a clot will occur. | ClinVar VCV000013310 (F2 c.*97G>A, established risk allele/risk factor) |
| rs113993960 | CFTR | (deletion) | CFTR Phe508del pathogenic deletion | The normalized three-base deletion (empty effect allele) is CFTR F508del. Carrier status depends on the genotype; this is not a diagnosis of cystic fibrosis. | ClinVar VCV000007105 (isolated CFTR c.1521_1523del, p.Phe508del, pathogenic, practice guideline) |
| rs4244285 | CYP2C19 | A | CYP2C19 *2 no-function allele | GRCh38 G>A is CYP2C19 c.681G>T on the reverse-strand transcript: *2, a no-function allele used with other alleles to infer metabolizer status. | CPIC CYP2C19 allele functionality; PharmGKB rs4244285; ClinVar VCV001703261 (drug response) |
| rs4986893 | CYP2C19 | A | CYP2C19 *3 no-function allele | A is CYP2C19 *3, a no-function allele; medication interpretation requires the full genotype, not this marker alone. | CPIC CYP2C19 allele functionality; PharmGKB rs4986893; ClinVar VCV000016899 (drug response) |
| rs12248560 | CYP2C19 | T | CYP2C19 *17 increased-function allele | T is the promoter variant defining CYP2C19 *17, an increased-function allele; metabolizer status depends on both haplotypes. | CPIC CYP2C19 allele functionality; PharmGKB rs12248560 (CYP2C19 *17, c.-806C>T) |
| rs1799853 | CYP2C9 | T | CYP2C9 *2 decreased-function allele | GRCh38 C>T corresponds to CYP2C9 c.430C>T, p.Arg144Cys (*2), a decreased-function allele. | CPIC CYP2C9 allele functionality; PharmGKB rs1799853 and PharmVar CYP2C9*2 |
| rs1057910 | CYP2C9 | C | CYP2C9 *3 decreased-function allele | GRCh38 A>C corresponds to CYP2C9 c.1075A>C, p.Ile359Leu (*3), a decreased-function allele. | CPIC CYP2C9 allele functionality; PharmGKB rs1057910 and PharmVar CYP2C9*3 |
| rs4149056 | SLCO1B1 | C | reduced transporter function; higher simvastatin-associated muscle side-effect risk | C is SLCO1B1 c.521T>C (Val174Ala), a decreased-function allele associated with simvastatin myopathy; effect size varies by genotype and medication. | CPIC statin-associated muscle symptoms guideline (SLCO1B1 c.521T>C); PharmGKB rs4149056; ClinVar VCV000037346 (reviewed by expert panel) |
| rs776746 | CYP3A5 | T | CYP3A5 *1 normal-function allele; C is the *3 decreased-function allele | GRCh38 T is CYP3A5 *1 (C on the reverse transcript is *3); *1 supports functional CYP3A5 expression. This is not a drug dose. | CPIC tacrolimus guideline; PharmGKB rs776746 (CYP3A5 *3A/*1 alleles) |
| rs4988235 | MCM6 | A | lactase persistence | GRCh38 G>A is LCT regulatory -13910C>T; A/T respectively are associated with lactase persistence, not a disease risk. | ClinVar VCV000007685 (LCT c.-13907C>T, lactase persistence); PMID:11788828 (Enattah et al., Nature Genetics 2002) |
| rs671 | ALDH2 | A | reduced ALDH2 function, alcohol flushing | A encodes ALDH2 E504K (historical E487K) and reduces enzyme activity, causing impaired acetaldehyde clearance and alcohol flushing. | ClinVar VCV000018390 (ALDH2 c.1510G>A, p.Glu504Lys, reviewed by expert panel) |

### Curation boundaries

rs6025 is **T**, not A, on the GRCh38 forward strand: T corresponds to the reverse-strand F5 transcript's Leiden c.1601G>A substitution. Panel ALT A is not Leiden.

APOE e4 requires rs429358 C **and** rs7412 C; rs429358 T with rs7412 T defines e2. The two rows carry haplotype wording, and dosage does not replace the existing two-site APOE result.

The CYP2C19, CYP2C9 and CYP3A5 marker counts are not diplotypes or prescribing instructions. Existing versioned PGx evaluation remains a separate path; its allele identities must be checked independently when this table changes. CPIC/PharmGKB references are labels for reviewer lookup, not a claim that generic dosage implements those guidelines.

HFE H63D is included as a well-known allele-definition row but ClinVar classifications conflict; its row expressly avoids claiming inevitable disease. CFTR F508del's normalized effect allele is the empty string (the three-base deletion), which `Some("")` deliberately distinguishes from `None`/unclear.

## Variant-by-variant GWAS audit

`old` is the first finding allele used by the old caller. `new` is the final panel allele. GWAS counts exclude the separate curated table.

| Variant | Gene | Card trait_group | Status | Old | New | Own-trait findings / eligible | Reason |
|---|---|---|---|---|---|---:|---|
| rs7903146 | TCF7L2 | Type 2 diabetes | **curated** | C | T | 49/7 | **Curated override**: higher type 2 diabetes risk. Source: PMID:17463248 (Scott et al., Science 2007; rs7903146 T2D association). |
| rs5219 | KCNJ11 | Type 2 diabetes | **unclear** | C | **unclear** | 7/0 | **Unclear**: 7 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. Explicit decreasing examples: C beta 0.0685 unit decrease. |
| rs1801282 | PPARG | Type 2 diabetes | **unclear** | C | **unclear** | 11/0 | **Unclear**: 11 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. Explicit decreasing examples: G beta 0.113 unit decrease; G beta 0.1134 unit decrease; G beta 0.1581 unit decrease. |
| rs13266634 | SLC30A8 | Type 2 diabetes | supported | C | C | 30/5 | Unchanged: unique supported allele; eligible evidence: 0.108 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog; 0.1004 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog; 0.101 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog; 0.126 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog; 0.1024 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog. |
| rs9939609 | FTO | Type 2 diabetes | **curated** | A | A | 2/0 | **Curated override**: higher type 2 diabetes risk mediated partly through BMI. Source: PMID:17434869 (Frayling et al., Science 2007). |
| rs7754840 | CDKAL1 | Type 2 diabetes | supported | C | C | 6/2 | Unchanged: unique supported allele; eligible evidence: 0.127 unit increase / PMID:28869590 / NHGRI-EBI GWAS catalog; 0.1226 unit increase / PMID:28869590 / NHGRI-EBI GWAS catalog. |
| rs4402960 | IGF2BP2 | Type 2 diabetes | **unclear** | T | **unclear** | 9/0 | **Unclear**: 9 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs1111875 | HHEX | Type 2 diabetes | supported | G | C | 14/2 | **Changed by H0a**: the old first-finding allele was not a supported increasing-risk allele; unique eligible allele; evidence: 0.0928 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog; 0.0923 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog. |
| rs1333049 | CDKN2B-AS1 | Heart and cholesterol | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs10455872 | LPA | Heart and cholesterol | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs3798220 | LPA | Heart and cholesterol | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs5742904 | APOB | Heart and cholesterol | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs11591147 | PCSK9 | Heart and cholesterol | **unclear** | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs6511720 | LDLR | Heart and cholesterol | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs699 | AGT | Blood pressure | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs429358 | APOE | Alzheimer (APOE type) | **curated** | C | C | 0/0 | **Curated override**: APOE e4-defining allele, only with rs7412 C. Source: ClinVar VCV000017864 (isolated APOE c.388T>C/rs429358 C allele); PharmGKB APOE allele definitions. |
| rs7412 | APOE | Alzheimer (APOE type) | **curated** | T | T | 0/0 | **Curated override**: APOE e2-defining allele, only with rs429358 T; not an e4 allele. Source: ClinVar VCV000017848 (isolated APOE c.526C>T/rs7412 T allele); PMID:7920638 (Corder et al., Nature Genetics 1994, APOE e2). |
| rs1061170 | CFH | Eye: macular degeneration | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs10490924 | ARMS2 | Eye: macular degeneration | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80338939 | GJB2 | Hearing loss | **unclear** | none | **unclear** | 2/0 | **Unclear**: 2 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs267606617 | MT-RNR1 | Hearing (antibiotic) | **unclear** | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1800562 | HFE | Iron overload | **curated** | A | A | 0/0 | **Curated override**: HFE-related hereditary hemochromatosis-associated C282Y variant. Source: ClinVar VCV000000009 (HFE c.845G>A, p.Cys282Tyr). |
| rs1799945 | HFE | Iron overload | **curated** | G | G | 0/0 | **Curated override**: HFE-related H63D variant; lower penetrance than C282Y, including in compound genotypes. Source: ClinVar VCV000000010 (HFE c.187C>G, p.His63Asp; record includes conflicting classifications). |
| rs6025 | F5 | Blood clotting | **curated** | T | T | 0/0 | **Curated override**: factor V Leiden-associated increased thrombosis risk. Source: ClinVar VCV000000642 (rs6025; F5 c.1601G>A, p.Arg534Gln); PMID:8164741 (Bertina et al., Nature 1994). |
| rs1799963 | F2 | Blood clotting | **curated** | G | A | 0/0 | **Curated override**: higher thrombosis risk. Source: ClinVar VCV000013310 (F2 c.*97G>A, established risk allele/risk factor). |
| rs28929474 | SERPINA1 | Lung/liver (AATD) | **unclear** | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs17580 | SERPINA1 | Lung/liver (AATD) | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80357914 | BRCA1 | Breast/ovarian | **unclear** | CTCTCT | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80357906 | BRCA1 | Breast/ovarian | **unclear** | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80359550 | BRCA2 | Breast/ovarian | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs555607708 | CHEK2 | Breast/other | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1801155 | APC | Colorectal | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs34612342 | MUTYH | Colorectal | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs36053993 | MUTYH | Colorectal | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs138213197 | HOXB13 | Prostate | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs334 | HBB | Sickle cell | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs113993960 | CFTR | Cystic fibrosis | **curated** | none |  | 2/0 | **Curated override**: CFTR Phe508del pathogenic deletion. Source: ClinVar VCV000007105 (isolated CFTR c.1521_1523del, p.Phe508del, pathogenic, practice guideline). |
| rs387906309 | HEXA | Tay-Sachs | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1050828 | G6PD | G6PD deficiency | **unclear** | T | **unclear** | 2/0 | **Unclear**: 2 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs4244285 | CYP2C19 | Clopidogrel, SSRIs | **curated** | A | A | 0/0 | **Curated override**: CYP2C19 *2 no-function allele. Source: CPIC CYP2C19 allele functionality; PharmGKB rs4244285; ClinVar VCV001703261 (drug response). |
| rs4986893 | CYP2C19 | Clopidogrel, SSRIs | **curated** | A | A | 0/0 | **Curated override**: CYP2C19 *3 no-function allele. Source: CPIC CYP2C19 allele functionality; PharmGKB rs4986893; ClinVar VCV000016899 (drug response). |
| rs12248560 | CYP2C19 | Clopidogrel, SSRIs | **curated** | T | T | 0/0 | **Curated override**: CYP2C19 *17 increased-function allele. Source: CPIC CYP2C19 allele functionality; PharmGKB rs12248560 (CYP2C19 *17, c.-806C>T). |
| rs1799853 | CYP2C9 | Warfarin | **curated** | T | T | 0/0 | **Curated override**: CYP2C9 *2 decreased-function allele. Source: CPIC CYP2C9 allele functionality; PharmGKB rs1799853 and PharmVar CYP2C9*2. |
| rs1057910 | CYP2C9 | Warfarin | **curated** | C | C | 0/0 | **Curated override**: CYP2C9 *3 decreased-function allele. Source: CPIC CYP2C9 allele functionality; PharmGKB rs1057910 and PharmVar CYP2C9*3. |
| rs9923231 | VKORC1 | Warfarin | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs4149056 | SLCO1B1 | Statins (muscle pain) | **curated** | C | C | 0/0 | **Curated override**: reduced transporter function; higher simvastatin-associated muscle side-effect risk. Source: CPIC statin-associated muscle symptoms guideline (SLCO1B1 c.521T>C); PharmGKB rs4149056; ClinVar VCV000037346 (reviewed by expert panel). |
| rs3918290 | DPYD | Fluorouracil | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1142345 | TPMT | Thiopurines | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs116855232 | NUDT15 | Thiopurines | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs776746 | CYP3A5 | Tacrolimus | **curated** | C | T | 0/0 | **Curated override**: CYP3A5 *1 normal-function allele; C is the *3 decreased-function allele. Source: CPIC tacrolimus guideline; PharmGKB rs776746 (CYP3A5 *3A/*1 alleles). |
| rs2395029 | HCP5 | Abacavir (HLA-B*57:01 tag) | **unclear** | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs4988235 | MCM6 | Lactose tolerance | **curated** | A | A | 0/0 | **Curated override**: lactase persistence. Source: ClinVar VCV000007685 (LCT c.-13907C>T, lactase persistence); PMID:11788828 (Enattah et al., Nature Genetics 2002). |
| rs671 | ALDH2 | Alcohol flush | **curated** | A | A | 0/0 | **Curated override**: reduced ALDH2 function, alcohol flushing. Source: ClinVar VCV000018390 (ALDH2 c.1510G>A, p.Glu504Lys, reviewed by expert panel). |
| rs1229984 | ADH1B | Alcohol metabolism | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs762551 | CYP1A2 | Caffeine metabolism | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs12913832 | HERC2 | Eye colour (blue/brown) | **unclear** | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1805007 | MC1R | Red hair / freckles | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs17822931 | ABCC11 | Earwax type | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1815739 | ACTN3 | Sprint vs endurance | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1426654 | SLC24A5 | Ancestry marker | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs16891982 | SLC45A2 | Ancestry marker | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs3827760 | EDAR | Ancestry marker | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs2814778 | ACKR1 | Ancestry marker | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1800414 | OCA2 | Ancestry marker | **unclear** | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs121434449 | HRURF/DMTN/BMP1 | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs121434450 | HRURF/DMTN/BMP1 | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs121913026 | RPS16P9/ERCC2/PPP1R13L | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1253128772 | CERS5/SMARCD1/GPD1 | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs137852782 | RNU6-857P/DSC3 | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs139426141 | MIR3972/PADI3/PADI1 | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs139876092 | MIR3972/PADI3/PADI1 | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs140482516 | MIR3972/PADI3/PADI1 | Hair loss (alopecia areata) | **unclear** | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1057518845 | SOX5 | Squint (strabismus) | **unclear** | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs6420484 | MIR6786/ARL16/CCDC137 | Squint (strabismus) | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs8070929 | Y_RNA/FAAP100/ACTG1 | Squint (strabismus) | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs8108303 | RPL39P34/ZNF137P/ZNF701 | Squint (strabismus) | **unclear** | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1996371 | RPL18P11/CHRNA3/CHRNB4 | Squint (strabismus) | **unclear** | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |

## Outcome

3 variants have a GWAS-supported effect allele, 18 are **curated**, and 56 remain **unclear** with no dosage. Before H0c, 4 were supported and 73 unclear. The underlying GWAS alleles remain unchanged; this includes H0a's changes at **rs7903146 (C → T)** and **rs1111875 (G → C)**. All 77 JSON effect statuses are covered by exact table/invariant tests. Genotypes, alternative alleles, read depths and truth-check concordance are not changed by curated annotations.

## Reproduction

Run `cargo run -p helixscan-panel -- audit-effects`. Rust embeds and validates `data/curated_effects.toml`, rewrites the effect fields in `data/panel.json`, and regenerates this document. No Python, JavaScript or network service evaluates effects. The tests cover every row plus decreasing, disagreeing, wrong-trait, missing-measure, unsupported-allele and OR boundary cases.
