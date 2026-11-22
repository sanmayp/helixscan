# Effect-allele audit

Source snapshot: `data/panel.json`, Ensembl release 116. This audit covers **all 77 variants**. The original Ensembl `risk_allele` values have not been overwritten.

## Conservative rule

Only findings whose `trait_text` equals the card's `trait_group` (case-insensitive) are eligible. Of those, only an odds ratio strictly greater than 1 or a beta containing `unit increase` carries an increasing-risk direction. Decreasing effects, other beta units, absent effect measures, ClinVar labels without an effect measure, gene names and allele frequencies do **not** select an effect allele. Multiple eligible alleles, a missing eligible allele, or an allele absent from the panel make it **unclear**. Unclear means `dosage = null` in the genotype report: no risk-allele copies are counted.

No eligible odds ratios are recorded in this Ensembl snapshot; retained alleles therefore come from own-trait beta `unit increase` findings. Trait labels such as `Heart and cholesterol` are not literal Ensembl traits, so no finding qualifies for those cards under this rule. APOE haplotypes and the separately versioned PGx interpretations do not depend on this generic risk dosage.

The report column is explicitly labelled `Unclear` when no allele is selected. Evidence remains available for inspection but does not imply an increasing effect. This is a provenance and direction audit, not clinical validation or an estimate of personal risk.

## Variant-by-variant result

`old` is the first finding allele used by the old caller. `new` is the audited allele.

| Variant | Gene | Card trait_group | Old | New | Own-trait findings / eligible | Reason |
|---|---|---|---|---|---:|---|
| rs7903146 | TCF7L2 | Type 2 diabetes | C | T | 49/7 | **Changed**: the old first-finding allele was not a supported increasing-risk allele; unique eligible allele; evidence: 0.228 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog; 0.301 unit increase / PMID:28869590 / NHGRI-EBI GWAS catalog; 0.3059 unit increase / PMID:30054458 / NHGRI-EBI GWAS catalog; 0.227926 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog; 0.1981 unit increase / PMID:28869590 / NHGRI-EBI GWAS catalog; 0.3286 unit increase / PMID:28869590 / NHGRI-EBI GWAS catalog; 0.251 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog. |
| rs5219 | KCNJ11 | Type 2 diabetes | C | **unclear** | 7/0 | **Unclear**: 7 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. Explicit decreasing examples: C beta 0.0685 unit decrease. |
| rs1801282 | PPARG | Type 2 diabetes | C | **unclear** | 11/0 | **Unclear**: 11 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. Explicit decreasing examples: G beta 0.113 unit decrease; G beta 0.1134 unit decrease; G beta 0.1581 unit decrease. |
| rs13266634 | SLC30A8 | Type 2 diabetes | C | C | 30/5 | Unchanged: unique supported allele; eligible evidence: 0.108 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog; 0.1004 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog; 0.101 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog; 0.126 unit increase / PMID:35551307 / NHGRI-EBI GWAS catalog; 0.1024 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog. |
| rs9939609 | FTO | Type 2 diabetes | A | **unclear** | 2/0 | **Unclear**: 2 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs7754840 | CDKAL1 | Type 2 diabetes | C | C | 6/2 | Unchanged: unique supported allele; eligible evidence: 0.127 unit increase / PMID:28869590 / NHGRI-EBI GWAS catalog; 0.1226 unit increase / PMID:28869590 / NHGRI-EBI GWAS catalog. |
| rs4402960 | IGF2BP2 | Type 2 diabetes | T | **unclear** | 9/0 | **Unclear**: 9 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs1111875 | HHEX | Type 2 diabetes | G | C | 14/2 | **Changed**: the old first-finding allele was not a supported increasing-risk allele; unique eligible allele; evidence: 0.0928 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog; 0.0923 unit increase / PMID:32541925 / NHGRI-EBI GWAS catalog. |
| rs1333049 | CDKN2B-AS1 | Heart and cholesterol | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs10455872 | LPA | Heart and cholesterol | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs3798220 | LPA | Heart and cholesterol | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs5742904 | APOB | Heart and cholesterol | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs11591147 | PCSK9 | Heart and cholesterol | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs6511720 | LDLR | Heart and cholesterol | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs699 | AGT | Blood pressure | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs429358 | APOE | Alzheimer (APOE type) | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs7412 | APOE | Alzheimer (APOE type) | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1061170 | CFH | Eye: macular degeneration | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs10490924 | ARMS2 | Eye: macular degeneration | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80338939 | GJB2 | Hearing loss | none | **unclear** | 2/0 | **Unclear**: 2 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs267606617 | MT-RNR1 | Hearing (antibiotic) | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1800562 | HFE | Iron overload | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1799945 | HFE | Iron overload | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs6025 | F5 | Blood clotting | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1799963 | F2 | Blood clotting | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs28929474 | SERPINA1 | Lung/liver (AATD) | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs17580 | SERPINA1 | Lung/liver (AATD) | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80357914 | BRCA1 | Breast/ovarian | CTCTCT | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80357906 | BRCA1 | Breast/ovarian | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs80359550 | BRCA2 | Breast/ovarian | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs555607708 | CHEK2 | Breast/other | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1801155 | APC | Colorectal | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs34612342 | MUTYH | Colorectal | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs36053993 | MUTYH | Colorectal | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs138213197 | HOXB13 | Prostate | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs334 | HBB | Sickle cell | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs113993960 | CFTR | Cystic fibrosis | none | **unclear** | 2/0 | **Unclear**: 2 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs387906309 | HEXA | Tay-Sachs | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1050828 | G6PD | G6PD deficiency | T | **unclear** | 2/0 | **Unclear**: 2 own-trait finding(s), none with OR &gt; 1 or beta `unit increase`. |
| rs4244285 | CYP2C19 | Clopidogrel, SSRIs | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs4986893 | CYP2C19 | Clopidogrel, SSRIs | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs12248560 | CYP2C19 | Clopidogrel, SSRIs | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1799853 | CYP2C9 | Warfarin | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1057910 | CYP2C9 | Warfarin | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs9923231 | VKORC1 | Warfarin | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs4149056 | SLCO1B1 | Statins (muscle pain) | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs3918290 | DPYD | Fluorouracil | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1142345 | TPMT | Thiopurines | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs116855232 | NUDT15 | Thiopurines | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs776746 | CYP3A5 | Tacrolimus | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs2395029 | HCP5 | Abacavir (HLA-B*57:01 tag) | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs4988235 | MCM6 | Lactose tolerance | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs671 | ALDH2 | Alcohol flush | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1229984 | ADH1B | Alcohol metabolism | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs762551 | CYP1A2 | Caffeine metabolism | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs12913832 | HERC2 | Eye colour (blue/brown) | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1805007 | MC1R | Red hair / freckles | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs17822931 | ABCC11 | Earwax type | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1815739 | ACTN3 | Sprint vs endurance | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1426654 | SLC24A5 | Ancestry marker | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs16891982 | SLC45A2 | Ancestry marker | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs3827760 | EDAR | Ancestry marker | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs2814778 | ACKR1 | Ancestry marker | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1800414 | OCA2 | Ancestry marker | C | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs121434449 | HRURF/DMTN/BMP1 | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs121434450 | HRURF/DMTN/BMP1 | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs121913026 | RPS16P9/ERCC2/PPP1R13L | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1253128772 | CERS5/SMARCD1/GPD1 | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs137852782 | RNU6-857P/DSC3 | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs139426141 | MIR3972/PADI3/PADI1 | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs139876092 | MIR3972/PADI3/PADI1 | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs140482516 | MIR3972/PADI3/PADI1 | Hair loss (alopecia areata) | none | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1057518845 | SOX5 | Squint (strabismus) | G | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs6420484 | MIR6786/ARL16/CCDC137 | Squint (strabismus) | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs8070929 | Y_RNA/FAAP100/ACTG1 | Squint (strabismus) | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs8108303 | RPL39P34/ZNF137P/ZNF701 | Squint (strabismus) | A | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |
| rs1996371 | RPL18P11/CHRNA3/CHRNB4 | Squint (strabismus) | T | **unclear** | 0/0 | **Unclear**: no findings have an exactly matching card trait_group; possible related traits are not assumed equivalent. |

## Outcome

4 variants have a supported effect allele; 73 are unclear and have no dosage. Variant alleles changed at **rs7903146 (C → T)** and **rs1111875 (G → C)**. Genotypes, alternative alleles and read depths are unchanged. This audit does not claim that unselected markers have no biological or medication effect; it means this evidence snapshot does not support generic increasing-risk allele counts under the stated rule.

## Reproduction

Run `cargo run -p helixscan-panel -- audit-effects`. It rewrites the audit fields in `data/panel.json` and regenerates this document. The tests construct decreasing, disagreeing, wrong-trait, missing-measure, unsupported-allele and OR boundary cases, and require rs7903146 = T in the actual 77-variant panel.
