//! Ağaç diyagramı yerleşimi. Önce soyut bir ağaç kurulur ve her düğüme kuşak (derinlik)
//! ile yaprak sırası verilir; sonra bu, dikey, yatay ya da dairesel koordinatlara
//! dönüştürülür. Koordinatlar düğüm merkezleridir.

use crate::db::{self, PersonInput};
use crate::kinship::Relationship;
use std::collections::{HashMap, HashSet};
use crate::{Result, Sex};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub const NODE_W: f32 = 180.0;
pub const NODE_H: f32 = 64.0;
pub const H_GAP: f32 = 24.0;
pub const V_GAP: f32 = 56.0;
/// Yatay görünümde kuşaklar arası ve satırlar arası mesafe
pub const COL_W: f32 = 240.0;
pub const ROW_H: f32 = 30.0;
/// Eşi gösterilen yatay görünümde satır aralığı (ad + altında eş satırı)
pub const ROW_H_SPOUSE: f32 = 48.0;
/// Dairesel görünümde halkalar arası mesafe
pub const RING: f32 = 130.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Projection {
    Vertical,
    Horizontal,
    Radial,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutNode {
    pub key: String,
    pub person_id: i64,
    pub label: String,
    /// "1895–1961" gibi
    pub years: String,
    pub sex: Option<Sex>,
    pub placeholder: bool,
    pub alive: Option<bool>,
    /// Soy görünümünde kişinin eşleri, akrabalık görünümünde ortak atanın eşi
    pub spouse_ids: Vec<i64>,
    pub spouse_labels: Vec<String>,
    pub depth: u32,
    pub x: f32,
    pub y: f32,
    /// Dairesel görünümde düğümün açısı (radyan); diğerlerinde 0
    pub angle: f32,
    pub repeated: bool,
    /// Görünümde birden çok kişiye bağlı (akraba evliliğinde iki koldan gelen ortak ata)
    pub shared: bool,
    /// Ortak atanın renk grubu (0, 1, 2…); arayüz her gruba ayrı renk verir
    pub shared_group: Option<u32>,
    /// İkiz kart: aynı kişinin asıl kartının anahtarı (ikiz kart modunda)
    pub twin_of: Option<String>,
    pub highlight: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEdge {
    pub from: String,
    pub to: String,
    pub highlight: bool,
    /// İkinci koldan ortak düğüme bağlanan çizgi (ağaç çizgisi değil)
    pub merge: bool,
    /// Ortak düğüme giden çizginin renk grubu (düğümün `shared_group`'u)
    pub group: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TreeLayout {
    pub projection: Projection,
    pub nodes: Vec<LayoutNode>,
    pub edges: Vec<LayoutEdge>,
    pub width: f32,
    pub height: f32,
}

/// Soyut ağaç düğümü.
struct TNode {
    person_id: i64,
    /// Ebeveyne göre rolü (metin görünümü için): "Baba", "Anne", "Oğlu", "Kızı"
    role: &'static str,
    spouse_ids: Vec<i64>,
    repeated: bool,
    children: Vec<TNode>,
}

fn years(p: &PersonInput) -> String {
    let year = |d: &str| d.rsplit(['/', '.']).next().filter(|y| y.len() == 4).unwrap_or("").to_string();
    let (b, d) = (year(&p.birth_date), year(&p.death_date));
    match (b.is_empty(), d.is_empty()) {
        (true, true) => String::new(),
        (false, true) => b,
        (true, false) => format!("?–{d}"),
        (false, false) => format!("{b}–{d}"),
    }
}

fn label(p: &PersonInput) -> String {
    let s = format!("{} {}", p.given_name, p.surname).trim().to_string();
    if s.is_empty() {
        "Bilinmiyor".into()
    } else {
        s
    }
}

/// Bir kişinin görünümdeki komşuları: yukarı (anne-baba) ya da aşağı (çocuklar).
fn next_people(conn: &Connection, id: i64, up: bool) -> Result<Vec<(i64, &'static str)>> {
    if up {
        let (f, m) = db::parents(conn, id)?;
        Ok([f.map(|x| (x, "Baba")), m.map(|x| (x, "Anne"))].into_iter().flatten().collect())
    } else {
        let mut out = Vec::new();
        for c in db::children(conn, id)? {
            let r = match db::get_person(conn, c)?.info.sex {
                Some(Sex::Female) => "Kızı",
                Some(Sex::Male) => "Oğlu",
                None => "Çocuğu",
            };
            out.push((c, r));
        }
        Ok(out)
    }
}

/// Kökten her kişiye giden en uzun yolun kuşak sayısı (`max_gen` sınırında). Akraba
/// evliliğinde bir ata farklı kollarda farklı uzaklıkta olabilir; en uzağı seçilir ki
/// tüm çizgiler aynı yöne gitsin. Her (kişi, kuşak) çifti bir kez gezilir.
fn longest_depths(conn: &Connection, root: i64, max_gen: u32, up: bool) -> Result<HashMap<i64, u32>> {
    let mut best: HashMap<i64, u32> = HashMap::new();
    let mut seen: HashSet<(i64, u32)> = HashSet::new();
    let mut stack = vec![(root, 0u32)];
    while let Some((id, d)) = stack.pop() {
        if !seen.insert((id, d)) {
            continue;
        }
        let b = best.entry(id).or_insert(d);
        *b = (*b).max(d);
        if d + 1 < max_gen {
            for (n, _) in next_people(conn, id, up)? {
                stack.push((n, d + 1));
            }
        }
    }
    Ok(best)
}

/// Ağaç kurar. Bir kişi yalnızca en uzun kuşak uzaklığındaki ilk geçişinde açılır; diğer
/// geçişler `repeated` yapraktır ve çizimde o tek düğüme bağlanır.
#[allow(clippy::too_many_arguments)]
fn build(
    conn: &Connection,
    id: i64,
    role: &'static str,
    depth: u32,
    max_gen: u32,
    up: bool,
    best: &HashMap<i64, u32>,
    expanded: &mut HashSet<i64>,
) -> Result<TNode> {
    let expand = best.get(&id) == Some(&depth) && expanded.insert(id);
    let mut spouse_ids = Vec::new();
    if !up {
        spouse_ids = db::spouses(conn, id)?;
    }
    let mut children = Vec::new();
    if expand && depth + 1 < max_gen {
        for (n, r) in next_people(conn, id, up)? {
            if !up {
                // Çocuğun diğer ebeveyni de eş sayılır
                let (f, m) = db::parents(conn, n)?;
                for other in [f, m].into_iter().flatten() {
                    if other != id && !spouse_ids.contains(&other) {
                        spouse_ids.push(other);
                    }
                }
            }
            children.push(build(conn, n, r, depth + 1, max_gen, up, best, expanded)?);
        }
    }
    Ok(TNode { person_id: id, role, spouse_ids, repeated: !expand, children })
}

fn tree(conn: &Connection, root: i64, max_gen: u32, up: bool) -> Result<TNode> {
    let max_gen = max_gen.max(1);
    let best = longest_depths(conn, root, max_gen, up)?;
    build(conn, root, "", 0, max_gen, up, &best, &mut HashSet::new())
}

/// Yerleştirilmiş düğüm: `layer` atalara doğru artı, torunlara doğru eksi kuşak.
struct Placed {
    key: String,
    person_id: i64,
    spouse_ids: Vec<i64>,
    layer: i32,
    order: f32,
    /// Hangi yarıda (kum saatinde atalar 0, torunlar 1; diğerlerinde 0)
    half: u8,
    /// İkiz kart (aynı kişinin ikinci geçişi; açılmaz)
    twin: bool,
}

/// Yapraklar ardışık sıra alır, iç düğüm ilk ve son çocuğunun ortasındadır (ayrık
/// aralıkların ortaları en az 1 birim uzak: aynı kuşakta çakışma olmaz). Tekrar eden
/// (açılmamış) çocuk, `twins` ise kendi yerinde ikiz kart olur; değilse yer kaplamaz ve
/// `cross`'a asıl düğüme giden bir birleşme bağı yazılır.
#[allow(clippy::too_many_arguments)]
fn place(
    n: &TNode,
    key: String,
    layer: i32,
    dir: i32,
    half: u8,
    next_leaf: &mut f32,
    out: &mut Vec<Placed>,
    edges: &mut Vec<(String, String)>,
    cross: &mut Vec<(String, i64, u8)>,
    twins: bool,
) -> f32 {
    let idx = out.len();
    out.push(Placed {
        key: key.clone(),
        person_id: n.person_id,
        spouse_ids: n.spouse_ids.clone(),
        layer,
        order: 0.0,
        half,
        twin: n.repeated,
    });
    let mut orders = Vec::new();
    for (i, c) in n.children.iter().enumerate() {
        if c.repeated && !twins {
            cross.push((key.clone(), c.person_id, half));
            continue;
        }
        let ck = format!("{key}.{i}");
        edges.push((key.clone(), ck.clone()));
        orders.push(place(c, ck, layer + dir, dir, half, next_leaf, out, edges, cross, twins));
    }
    let order = if orders.is_empty() {
        let o = *next_leaf;
        *next_leaf += 1.0;
        o
    } else {
        (orders[0] + orders[orders.len() - 1]) / 2.0
    };
    out[idx].order = order;
    order
}

/// Yerleşimi koordinatlara çevirir. `hourglass`: atalar ve torunlar aynı çizimde.
fn project(
    conn: &Connection,
    placed: Vec<Placed>,
    mut edges: Vec<(String, String)>,
    cross: Vec<(String, i64, u8)>,
    leaves: [f32; 2],
    proj: Projection,
    hourglass: bool,
    highlight: bool,
) -> Result<TreeLayout> {
    // Tekrar eden geçişleri aynı yarıdaki tek düğüme bağla
    let mut merges: HashSet<(String, String)> = HashSet::new();
    for (from, pid, half) in cross {
        if let Some(t) = placed.iter().find(|p| p.person_id == pid && !p.twin && (p.half == half || p.layer == 0)) {
            merges.insert((from.clone(), t.key.clone()));
            edges.push((from, t.key.clone()));
        }
    }
    let max_layer = placed.iter().map(|p| p.layer).max().unwrap_or(0) as f32;
    // Yatay görünümde eş adı adın altına ayrı satır olarak çizilir
    let row = if placed.iter().any(|p| !p.spouse_ids.is_empty()) { ROW_H_SPOUSE } else { ROW_H };
    let mut nodes = Vec::with_capacity(placed.len());
    for p in &placed {
        let layer = p.layer as f32;
        let (x, y, angle) = match proj {
            Projection::Vertical => (p.order * (NODE_W + H_GAP), (max_layer - layer) * (NODE_H + V_GAP), 0.0),
            Projection::Horizontal => {
                let col = if hourglass { layer } else { layer.abs() };
                (col * COL_W, p.order * row, 0.0)
            }
            Projection::Radial => {
                let a = if hourglass {
                    // Atalar üst yarıda, torunlar alt yarıda
                    let n = leaves[p.half as usize].max(1.0);
                    let t = (p.order + 0.5) / n * std::f32::consts::PI;
                    if p.half == 0 { std::f32::consts::PI + t } else { std::f32::consts::PI - t }
                } else if leaves[0] <= 1.0 {
                    0.0
                } else {
                    (p.order + 0.5) / leaves[0] * std::f32::consts::TAU
                };
                let a = if p.layer == 0 { 0.0 } else { a };
                let r = layer.abs() * RING;
                (r * a.cos(), r * a.sin(), a)
            }
        };
        let info = db::get_person(conn, p.person_id)?;
        let mut spouse_labels = Vec::new();
        for s in &p.spouse_ids {
            spouse_labels.push(label(&db::get_person(conn, *s)?.info));
        }
        nodes.push(LayoutNode {
            key: p.key.clone(),
            person_id: p.person_id,
            label: label(&info.info),
            years: years(&info.info),
            sex: info.info.sex,
            placeholder: info.placeholder,
            alive: info.info.alive,
            spouse_ids: p.spouse_ids.clone(),
            spouse_labels,
            depth: p.layer.unsigned_abs(),
            x,
            y,
            angle,
            repeated: false,
            shared: false,
            shared_group: None,
            twin_of: if p.twin {
                placed
                    .iter()
                    .find(|q| q.person_id == p.person_id && !q.twin && (q.half == p.half || q.layer == 0))
                    .map(|q| q.key.clone())
            } else {
                None
            },
            highlight,
        });
    }
    // Ortak kişi: iki koldan bağlanan (birleşik düğüm) ya da ikiz kartı olan; aynı kişinin
    // tüm kartları aynı renk grubunda
    let mut group_of_person: HashMap<i64, u32> = HashMap::new();
    for n in &nodes {
        let multi = edges.iter().filter(|(_, to)| *to == n.key).count() > 1;
        if (multi || n.twin_of.is_some()) && !group_of_person.contains_key(&n.person_id) {
            let g = group_of_person.len() as u32;
            group_of_person.insert(n.person_id, g);
        }
    }
    // Grup numarası ağaçtaki ilk görünüş sırasına göre (renkler kararlı olsun)
    let mut order: Vec<(usize, i64)> = Vec::new();
    for (i, n) in nodes.iter().enumerate() {
        if group_of_person.contains_key(&n.person_id) && !order.iter().any(|(_, p)| *p == n.person_id) {
            order.push((i, n.person_id));
        }
    }
    for (g, (_, pid)) in order.iter().enumerate() {
        group_of_person.insert(*pid, g as u32);
    }
    for n in nodes.iter_mut() {
        n.shared_group = group_of_person.get(&n.person_id).copied();
        n.shared = n.shared_group.is_some();
    }
    // Sol üst köşeye kenar boşluğuyla taşı
    let (mx, my, extra_w, extra_h) = match proj {
        Projection::Vertical => (NODE_W / 2.0, NODE_H / 2.0, NODE_W / 2.0, NODE_H / 2.0),
        Projection::Horizontal => (ROW_H, ROW_H, COL_W, ROW_H),
        Projection::Radial => (RING, RING, RING, RING),
    };
    let min_x = nodes.iter().map(|n| n.x).fold(f32::INFINITY, f32::min);
    let min_y = nodes.iter().map(|n| n.y).fold(f32::INFINITY, f32::min);
    for n in nodes.iter_mut() {
        n.x += mx - min_x;
        n.y += my - min_y;
    }
    let width = nodes.iter().map(|n| n.x).fold(0.0, f32::max) + extra_w;
    let height = nodes.iter().map(|n| n.y).fold(0.0, f32::max) + extra_h;
    let group_of: HashMap<&str, u32> =
        nodes.iter().filter_map(|n| n.shared_group.map(|g| (n.key.as_str(), g))).collect();
    let edges = edges
        .into_iter()
        .map(|(from, to)| LayoutEdge {
            merge: merges.contains(&(from.clone(), to.clone())),
            group: group_of.get(to.as_str()).copied(),
            from,
            to,
            highlight,
        })
        .collect();
    Ok(TreeLayout { projection: proj, nodes, edges, width, height })
}

/// Tek yönlü ağaç(lar) için yerleşim. `dir`: +1 atalar, -1 torunlar.
fn layout_trees(conn: &Connection, roots: &[TNode], dir: i32, proj: Projection, highlight: bool, twins: bool) -> Result<TreeLayout> {
    let (mut placed, mut edges, mut cross) = (Vec::new(), Vec::new(), Vec::new());
    let mut next_leaf = 0.0;
    for (i, r) in roots.iter().enumerate() {
        place(r, format!("n{i}"), 0, dir, 0, &mut next_leaf, &mut placed, &mut edges, &mut cross, twins);
    }
    project(conn, placed, edges, cross, [next_leaf, 0.0], proj, false, highlight)
}

/// Ata ağacı; `max_gen` kök dahil kuşak sayısı. Akraba evliliğindeki ortak ata `twins`
/// ise iki kolda da yerinde (asıl kart + ikiz kart), değilse tek düğümdür ve ikinci koldan
/// gelen çizgi (`merge`) ona bağlanır. Her iki durumda da `shared` ve renk grubu verilir.
pub fn ancestors_layout(conn: &Connection, root: i64, max_gen: u32, proj: Projection, twins: bool) -> Result<TreeLayout> {
    layout_trees(conn, &[tree(conn, root, max_gen, true)?], 1, proj, false, twins)
}

/// Soy ağacı; eşler düğümün yan bilgisidir. İki ebeveyni de görünümde olan çocuk ortak kişidir.
pub fn descendants_layout(conn: &Connection, root: i64, max_gen: u32, proj: Projection, twins: bool) -> Result<TreeLayout> {
    layout_trees(conn, &[tree(conn, root, max_gen, false)?], -1, proj, false, twins)
}

/// Kum saati: kişi ortada, ataları bir yanda, torunları öbür yanda (`gens` her iki yöne).
pub fn hourglass_layout(conn: &Connection, root: i64, gens: u32, proj: Projection, twins: bool) -> Result<TreeLayout> {
    let up = tree(conn, root, gens, true)?;
    let mut down = tree(conn, root, gens, false)?;
    let (mut placed, mut edges, mut cross) = (Vec::new(), Vec::new(), Vec::new());
    let mut leaves_up = 0.0;
    place(&up, "a".into(), 0, 1, 0, &mut leaves_up, &mut placed, &mut edges, &mut cross, twins);
    let root_order = placed[0].order;
    // Torun yarısı: kökü atalar yarısındaki kökle aynı düğüm
    let spouse_ids = std::mem::take(&mut down.spouse_ids);
    placed[0].spouse_ids = spouse_ids;
    let (mut p2, mut e2, mut c2) = (Vec::new(), Vec::new(), Vec::new());
    let mut leaves_down = 0.0;
    place(&down, "d".into(), 0, -1, 1, &mut leaves_down, &mut p2, &mut e2, &mut c2, twins);
    let shift = if proj == Projection::Radial { 0.0 } else { root_order - p2[0].order };
    let rename = |k: String| if k == "d" { "a".to_string() } else { k };
    placed.extend(p2.into_iter().skip(1).map(|mut p| {
        p.order += shift;
        p
    }));
    edges.extend(e2.into_iter().map(|(a, b)| (rename(a), rename(b))));
    cross.extend(c2.into_iter().map(|(a, pid, h)| (rename(a), pid, h)));
    project(conn, placed, edges, cross, [leaves_up, leaves_down], proj, true, false)
}

/// İki kişinin en yakın ortak ata(lar)ından kendilerine inen yollar. Aynı çocuğa inen
/// ortak atalar (karı-koca) tek tepe düğümünde birleşir.
pub fn relationship_layout(conn: &Connection, rel: &Relationship, proj: Projection) -> Result<TreeLayout> {
    let mut roots: Vec<TNode> = Vec::new();
    let mut tops: Vec<(Option<i64>, Option<i64>)> = Vec::new();
    for ca in &rel.closest {
        // Ataların hemen altındaki kişiler (yoksa kişinin kendisi atadır)
        let below = (ca.path_a.iter().rev().nth(1).copied(), ca.path_b.iter().rev().nth(1).copied());
        if let Some(i) = tops.iter().position(|t| *t == below) {
            roots[i].spouse_ids.push(ca.ancestor);
            continue;
        }
        let chain = |path: &[i64]| -> Option<TNode> {
            // path: kişi → ata; ata hariç, yukarıdan aşağı zincir
            let mut node: Option<TNode> = None;
            for &id in path.iter().take(path.len().saturating_sub(1)) {
                node = Some(TNode { person_id: id, role: "", spouse_ids: Vec::new(), repeated: false, children: node.into_iter().collect() });
            }
            node
        };
        let children: Vec<TNode> = [chain(&ca.path_a), chain(&ca.path_b)].into_iter().flatten().collect();
        roots.push(TNode { person_id: ca.ancestor, role: "", spouse_ids: Vec::new(), repeated: false, children });
        tops.push(below);
    }
    layout_trees(conn, &roots, -1, proj, true, false)
}

fn write_outline(conn: &Connection, n: &TNode, depth: usize, out: &mut String) -> Result<()> {
    let p = db::get_person(conn, n.person_id)?;
    out.push_str(&"*".repeat(depth + 1));
    out.push(' ');
    if !n.role.is_empty() {
        out.push_str(n.role);
        out.push_str(": ");
    }
    out.push_str(&label(&p.info));
    let y = years(&p.info);
    if !y.is_empty() {
        out.push_str(&format!(" ({y})"));
    }
    if n.repeated {
        out.push_str(" ↻");
    }
    out.push('\n');
    for c in &n.children {
        write_outline(conn, c, depth + 1, out)?;
    }
    Ok(())
}

/// Girintili metin görünümü: "* EMRE YILMAZ", "** Baba: MEHMET YILMAZ (1958)" … Metinde
/// ortak ata birleştirilemez; ikinci geçişi "↻" ile işaretlenir.
pub fn outline(conn: &Connection, root: i64, max_gen: u32, ancestors: bool) -> Result<String> {
    let t = tree(conn, root, max_gen, ancestors)?;
    let mut out = String::new();
    write_outline(conn, &t, 0, &mut out)?;
    Ok(out)
}
