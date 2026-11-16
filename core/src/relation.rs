//! "Babasının Annesinin Babası" gibi yakınlık ifadelerinden aile grafiği.
//!
//! İfadeler PDF sahibine (kök kişiye) göredir; burada yalnızca kimin kimin ebeveyni,
//! eşi olduğu çıkarılır ve ifadenin kendisi bir daha kullanılmaz.

use crate::{tr_fold, Error, Record, Result, Sex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    Father,
    Mother,
    Son,
    Daughter,
    Spouse,
    Sibling,
}

impl Step {
    /// Bu adımla ulaşılan kişinin cinsiyeti (eş ve kardeş için bilinmez).
    pub fn sex(self) -> Option<Sex> {
        match self {
            Step::Father | Step::Son => Some(Sex::Male),
            Step::Mother | Step::Daughter => Some(Sex::Female),
            Step::Spouse | Step::Sibling => None,
        }
    }
}

/// "Babasının Annesinin Babası" → `[Father, Mother, Father]`; "Kendisi" → `[]`.
pub fn parse_relation(s: &str) -> Result<Vec<Step>> {
    let folded = tr_fold(s);
    if folded == "kendisi" {
        return Ok(Vec::new());
    }
    folded
        .split(' ')
        .map(|w| match w {
            "babası" | "babasının" => Ok(Step::Father),
            "annesi" | "annesinin" => Ok(Step::Mother),
            "oğlu" | "oğlunun" => Ok(Step::Son),
            "kızı" | "kızının" => Ok(Step::Daughter),
            "eşi" | "eşinin" => Ok(Step::Spouse),
            "kardeşi" | "kardeşinin" => Ok(Step::Sibling),
            _ => Err(Error::Invalid(format!("Tanınmayan yakınlık: {}", s.trim()))),
        })
        .collect()
}

/// Grafikteki bir kişi; `record` yoksa PDF'te satırı olmayan yer tutucudur.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub record: Option<Record>,
    pub path: Vec<Step>,
    pub sex: Option<Sex>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordGraph {
    pub nodes: Vec<GraphNode>,
    /// (çocuk, ebeveyn, ebeveynin rolü)
    pub parent_edges: Vec<(usize, usize, Sex)>,
    pub spouse_edges: Vec<(usize, usize)>,
    /// Bir önceki kişisi birden çok adaydan seçilemediği için bağlanamayan düğümler
    pub unlinked: Vec<usize>,
}

impl RecordGraph {
    fn find(&self, path: &[Step]) -> Option<usize> {
        self.nodes.iter().position(|n| n.path == path)
    }

    /// Yol için düğüm döndürür; yoksa (ve öneki yoksa özyinelemeli olarak) yer tutucu ekler.
    fn ensure(&mut self, path: &[Step]) -> usize {
        if let Some(i) = self.find(path) {
            return i;
        }
        if let Some((_, prefix)) = path.split_last() {
            self.ensure(prefix);
        }
        self.nodes.push(GraphNode {
            record: None,
            path: path.to_vec(),
            sex: path.last().and_then(|s| s.sex()),
        });
        self.nodes.len() - 1
    }
}

/// Bir PDF'in kayıtlarından aile grafiğini kurar. Satırı olmayan ara kişiler
/// (ör. "Babasının Annesi" yokken "Babasının Annesinin Babası") yer tutucu olur.
///
/// Aynı yakınlık ifadesi birden çok satırda geçebilir (iki "Kızı", iki "Kardeşi"). Bir
/// satırın bir önceki kişisi (ör. "Kızının Oğlu" için hangi "Kızı") birden çok adaydan
/// seçilmesi gerekirse satırdaki baba/ana adına bakılır; yine seçilemezse satır
/// bağlanmaz ve `unlinked`'e eklenir.
pub fn build_graph(records: &[Record]) -> Result<RecordGraph> {
    let mut g = RecordGraph::default();
    for r in records {
        let path = parse_relation(&r.relation)?;
        g.nodes.push(GraphNode {
            sex: r.sex.or_else(|| path.last().and_then(|s| s.sex())),
            record: Some(r.clone()),
            path,
        });
    }
    g.ensure(&[]);
    let paths: Vec<Vec<Step>> = g.nodes.iter().map(|n| n.path.clone()).collect();
    for p in &paths {
        if let Some((_, prefix)) = p.split_last() {
            g.ensure(prefix);
        }
    }
    let name_eq = |a: &str, b: &str| !a.is_empty() && tr_fold(a) == tr_fold(b);
    for i in 0..g.nodes.len() {
        let path = g.nodes[i].path.clone();
        let Some((last, prefix)) = path.split_last() else { continue };
        let candidates: Vec<usize> = (0..g.nodes.len()).filter(|&j| j != i && g.nodes[j].path == prefix).collect();
        let me = g.nodes[i].record.clone();
        // Önceki kişi: tek adaysa o; değilse adlarla seç
        let p = if candidates.len() == 1 {
            Some(candidates[0])
        } else {
            let fits: Vec<usize> = candidates
                .iter()
                .copied()
                .filter(|&j| {
                    let other = g.nodes[j].record.as_ref();
                    match (last, &me, other) {
                        // Ben, adayın babası/annesiyim: adayın baba/ana adı benim adım
                        (Step::Father, Some(m), Some(o)) => name_eq(&o.father_name, &m.given_name),
                        (Step::Mother, Some(m), Some(o)) => name_eq(&o.mother_name, &m.given_name),
                        // Ben, adayın çocuğuyum: benim baba/ana adım adayın adı
                        (Step::Son | Step::Daughter, Some(m), Some(o)) => match g.nodes[j].sex {
                            Some(Sex::Male) => name_eq(&m.father_name, &o.given_name),
                            Some(Sex::Female) => name_eq(&m.mother_name, &o.given_name),
                            None => name_eq(&m.father_name, &o.given_name) || name_eq(&m.mother_name, &o.given_name),
                        },
                        _ => false,
                    }
                })
                .collect();
            if fits.len() == 1 {
                Some(fits[0])
            } else {
                None
            }
        };
        let Some(p) = p else {
            g.unlinked.push(i);
            continue;
        };
        match last {
            Step::Father => g.parent_edges.push((p, i, Sex::Male)),
            Step::Mother => g.parent_edges.push((p, i, Sex::Female)),
            Step::Son | Step::Daughter => {
                let role = g.nodes[p].sex.or_else(|| prefix.last().and_then(|s| s.sex()));
                if let Some(role) = role {
                    g.parent_edges.push((i, p, role));
                }
            }
            Step::Spouse => g.spouse_edges.push((p, i)),
            Step::Sibling => {
                for (step, role) in [(Step::Father, Sex::Male), (Step::Mother, Sex::Female)] {
                    let mut pp = prefix.to_vec();
                    pp.push(step);
                    if let Some(parent) = g.find(&pp) {
                        g.parent_edges.push((i, parent, role));
                    }
                }
            }
        }
    }
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Step::*;

    fn rec(relation: &str, sex: Sex, name: &str) -> Record {
        Record { relation: relation.into(), sex: Some(sex), given_name: name.into(), ..Default::default() }
    }

    fn idx(g: &RecordGraph, path: &[Step]) -> usize {
        g.nodes.iter().position(|n| n.path == path).expect("düğüm")
    }

    #[test]
    fn parse_relation_examples() {
        assert_eq!(parse_relation("Babasının Annesinin Babası").unwrap(), vec![Father, Mother, Father]);
        assert_eq!(parse_relation("KENDİSİ").unwrap(), vec![]);
        assert_eq!(parse_relation("Oğlunun Kızı").unwrap(), vec![Son, Daughter]);
        assert_eq!(parse_relation("Eşinin Annesi").unwrap(), vec![Spouse, Mother]);
        assert_eq!(parse_relation("Kardeşi").unwrap(), vec![Sibling]);
        assert_eq!(
            parse_relation("Babasının Annesinin  Annesinin Annesi").unwrap(),
            vec![Father, Mother, Mother, Mother]
        );
        match parse_relation("Komşusu") {
            Err(Error::Invalid(m)) => assert!(m.contains("Komşusu"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn graph_links_parents() {
        let g = build_graph(&[
            rec("Kendisi", Sex::Male, "EMRE"),
            rec("Babası", Sex::Male, "MEHMET"),
            rec("Annesi", Sex::Female, "NURTEN"),
            rec("Babasının Babası", Sex::Male, "KADİR"),
        ])
        .unwrap();
        assert_eq!(g.nodes.len(), 4);
        let (root, f, m, ff) = (idx(&g, &[]), idx(&g, &[Father]), idx(&g, &[Mother]), idx(&g, &[Father, Father]));
        let mut e = g.parent_edges.clone();
        e.sort();
        let mut want = vec![(root, f, Sex::Male), (root, m, Sex::Female), (f, ff, Sex::Male)];
        want.sort();
        assert_eq!(e, want);
    }

    #[test]
    fn gap_creates_placeholder() {
        let g = build_graph(&[
            rec("Kendisi", Sex::Female, "AYŞE"),
            rec("Babasının Annesinin Babası", Sex::Male, "SADIK"),
        ])
        .unwrap();
        assert_eq!(g.nodes.len(), 4);
        let f = idx(&g, &[Father]);
        let fm = idx(&g, &[Father, Mother]);
        assert!(g.nodes[f].record.is_none() && g.nodes[fm].record.is_none());
        assert_eq!(g.nodes[f].sex, Some(Sex::Male));
        assert_eq!(g.nodes[fm].sex, Some(Sex::Female));
        let fmf = idx(&g, &[Father, Mother, Father]);
        assert!(g.parent_edges.contains(&(fm, fmf, Sex::Male)));
        assert!(g.parent_edges.contains(&(f, fm, Sex::Female)));
        assert!(g.parent_edges.contains(&(idx(&g, &[]), f, Sex::Male)));
    }

    #[test]
    fn child_row_links_to_root() {
        let g = build_graph(&[rec("Kendisi", Sex::Female, "AYŞE"), rec("Oğlu", Sex::Male, "CAN")]).unwrap();
        assert_eq!(g.parent_edges, vec![(idx(&g, &[Son]), idx(&g, &[]), Sex::Female)]);
    }

    #[test]
    fn spouse_and_sibling() {
        let g = build_graph(&[
            rec("Kendisi", Sex::Male, "EMRE"),
            rec("Babası", Sex::Male, "MEHMET"),
            rec("Eşi", Sex::Female, "SELİN"),
            rec("Kardeşi", Sex::Female, "NUR"),
        ])
        .unwrap();
        assert_eq!(g.spouse_edges, vec![(idx(&g, &[]), idx(&g, &[Spouse]))]);
        assert!(g.parent_edges.contains(&(idx(&g, &[Sibling]), idx(&g, &[Father]), Sex::Male)));
    }

    fn rec_with(relation: &str, sex: Sex, name: &str, father: &str, mother: &str) -> Record {
        Record { father_name: father.into(), mother_name: mother.into(), ..rec(relation, sex, name) }
    }

    fn by_name(g: &RecordGraph, name: &str) -> usize {
        g.nodes.iter().position(|n| n.record.as_ref().is_some_and(|r| r.given_name == name)).unwrap()
    }

    /// Alt soy satırları: iki kız ve birinin oğlu. Aynı yakınlık ifadesi iki kez geçer.
    #[test]
    fn two_daughters_and_grandchild() {
        let g = build_graph(&[
            rec_with("Kendisi", Sex::Female, "AYŞE", "OSMAN", "GÜL"),
            rec_with("Kızı", Sex::Female, "NUR", "KEMAL", "AYŞE"),
            rec_with("Kızı", Sex::Female, "SU", "KEMAL", "AYŞE"),
            rec_with("Kızının Oğlu", Sex::Male, "CAN", "TOLGA", "SU"),
        ])
        .unwrap();
        assert_eq!(g.nodes.len(), 4);
        let root = idx(&g, &[]);
        assert!(g.parent_edges.contains(&(by_name(&g, "NUR"), root, Sex::Female)));
        assert!(g.parent_edges.contains(&(by_name(&g, "SU"), root, Sex::Female)));
        assert!(g.parent_edges.contains(&(by_name(&g, "CAN"), by_name(&g, "SU"), Sex::Female)));
        assert!(g.unlinked.is_empty());
    }

    /// Hangi kızın çocuğu olduğu anlaşılamazsa satır bağlanmaz ama içe aktarma durmaz.
    #[test]
    fn ambiguous_parent_left_unlinked() {
        let g = build_graph(&[
            rec_with("Kendisi", Sex::Female, "AYŞE", "", ""),
            rec_with("Kızı", Sex::Female, "NUR", "", "AYŞE"),
            rec_with("Kızı", Sex::Female, "SU", "", "AYŞE"),
            rec_with("Kızının Oğlu", Sex::Male, "CAN", "", "BAŞKA"),
        ])
        .unwrap();
        let can = by_name(&g, "CAN");
        assert_eq!(g.unlinked, vec![can]);
        assert!(!g.parent_edges.iter().any(|e| e.0 == can || e.1 == can));
    }

    #[test]
    fn missing_root_becomes_placeholder() {
        let g = build_graph(&[rec("Babası", Sex::Male, "MEHMET")]).unwrap();
        assert!(g.nodes[idx(&g, &[])].record.is_none());
    }
}
