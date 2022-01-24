use std::{default, iter::FromIterator, ops::Sub, vec};

pub use indexmap::{IndexMap, IndexSet};
use numpy::ndarray::s;
use serde::{de::IntoDeserializer, Deserialize, Serialize};
use tree_sitter::{Node, QueryMatch};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Tangle {
    Dataflow {
        name: String,
        nodes: IndexMap<String, Tangle>,
        provides: IndexSet<String>,
        requires: IndexSet<String>,
    },
    Leaf {
        name: String,
        provides: IndexSet<String>,
        requires: IndexSet<String>,
        code: String,
    },
}

use std::sync::atomic::{AtomicUsize, Ordering};

static NAMER: std::sync::atomic::AtomicUsize = AtomicUsize::new(0);

impl Default for Tangle {
    fn default() -> Self {
        let name = NAMER.fetch_add(1, Ordering::SeqCst);
        Self::Leaf {
            name: format!("tangle_{}", name),
            provides: Default::default(),
            requires: Default::default(),
            code: "".to_string(),
        }
    }
}

impl Tangle {
    pub fn name(&self) -> &String {
        match self {
            Self::Dataflow { name, .. } => name,
            Self::Leaf { name, .. } => name,
        }
    }

    pub fn deps(&self) -> (&IndexSet<String>, &IndexSet<String>) {
        match self {
            Self::Dataflow {
                requires, provides, ..
            } => (requires, provides),
            Self::Leaf {
                requires, provides, ..
            } => (requires, provides),
        }
    }

    pub fn code(&self) -> &String {
        match self {
            Self::Leaf { code, .. } => code,
            Self::Dataflow { .. } => panic!(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    MissingLeaf(String),
    NoDataflowFound,
    RequiresMismatch {
        expected: IndexSet<String>,
        found: IndexSet<String>,
    },
    ProvidesMismatch {
        expected: IndexSet<String>,
        found: IndexSet<String>,
    },
}

lazy_static! {
    static ref QUERY: tree_sitter::Query = {
        tree_sitter::Query::new(tree_sitter_python::language(), include_str!("query.scm")).unwrap()
    };
    static ref CAPTURE_NAMES: &'static [String] = QUERY.capture_names();
    static ref LEAF_NAME: u32 = QUERY.capture_index_for_name("leaf.name").unwrap();
    static ref LEAF_PARAM: u32 = QUERY.capture_index_for_name("leaf.param").unwrap();
    static ref LEAF_BODY: u32 = QUERY.capture_index_for_name("leaf.body").unwrap();
    static ref LEAF_PROVIDES: u32 = QUERY.capture_index_for_name("leaf.provides").unwrap();
    // static ref FLOW_NAME: u32 = QUERY.capture_index_for_name("flow.name").unwrap();
    // static ref FLOW_PARAM: u32 = QUERY.capture_index_for_name("flow.param").unwrap();
    // static ref FLOW_PROVIDES: u32 = QUERY.capture_index_for_name("flow.provides").unwrap();
    static ref FLOW_NODE: u32 = QUERY.capture_index_for_name("flow.node").unwrap();
    // static ref FLOW_NODE_NAME: u32 = QUERY.capture_index_for_name("flow.node.name").unwrap();
    // static ref FLOW_NODE_PARAM: u32 = QUERY.capture_index_for_name("flow.node.arg").unwrap();
    // static ref FLOW_NODE_PROVIDES: u32 =
    //     QUERY.capture_index_for_name("flow.node.provides").unwrap();

    static ref NODE_QUERY: tree_sitter::Query = {
        tree_sitter::Query::new(tree_sitter_python::language(), include_str!("node_query.scm")).unwrap()
    };

    static ref FLOW_NODE_NAME: u32 = NODE_QUERY.capture_index_for_name("flow.node.name").unwrap();
    static ref FLOW_NODE_PARAM: u32 = NODE_QUERY.capture_index_for_name("flow.node.param").unwrap();
    static ref FLOW_NODE_PROVIDES: u32 =
    NODE_QUERY.capture_index_for_name("flow.node.provides").unwrap();

    static ref CELL_QUERY: tree_sitter::Query = {
        tree_sitter::Query::new(tree_sitter_python::language(), include_str!("cell_query.scm")).unwrap()
    };
    static ref CELL_FUNCDEF: u32 = CELL_QUERY.capture_index_for_name("function").unwrap();
    static ref CELL_VARIABLE: u32 = CELL_QUERY.capture_index_for_name("variable").unwrap();
    static ref CELL_IMPORT: u32 = CELL_QUERY.capture_index_for_name("import").unwrap();
    static ref CELL_IDENTIFIER: u32 = CELL_QUERY.capture_index_for_name("identifier").unwrap();
    static ref CELL_ERROR: u32 = CELL_QUERY.capture_index_for_name("error").unwrap();
}

#[derive(Clone, Debug, Default)]
pub struct TangleString<'a>(&'a str);

impl<'a> tree_sitter::TextProvider<'a> for TangleString<'a> {
    type I = core::iter::Once<&'a [u8]>;

    fn text(&mut self, node: tree_sitter::Node) -> Self::I {
        core::iter::once(&self.0[node.byte_range()].as_bytes())
    }
}

impl<'a> From<&'a str> for TangleString<'a> {
    fn from(s: &'a str) -> Self {
        Self(s)
    }
}

fn parse_id_or_seq(capture_idx: u32, qm: &QueryMatch, code: &TangleString) -> IndexSet<String> {
    if let Some(node) = qm.nodes_for_capture_index(capture_idx).next() {
        if node.kind() == "list"
            || node.kind() == "parameters"
            || node.kind() == "list_pattern"
            || node.kind() == "argument_list"
        {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .map(|n| code.0[n.byte_range()].to_string())
                .collect()
        } else {
            vec![code.0[node.byte_range()].to_string()]
                .into_iter()
                .collect()
        }
    } else {
        Default::default()
    }
}

fn singleton(leaf: &Tangle) -> Tangle {
    match leaf.clone() {
        Tangle::Leaf {
            name,
            requires,
            provides,
            ..
        } => Tangle::Dataflow {
            name: name.clone(),
            requires,
            provides,
            nodes: IndexMap::from_iter(vec![(name.clone(), leaf.clone())].into_iter()),
        },
        flow => flow,
    }
}

// fn merge(d1: &Tangle, d2: &Tangle) -> Tangle {
//     match (d1, d2) {
//         (Tangle::Leaf { .. }, _) => merge(&singleton(d1), d2),
//         (_, Tangle::Leaf { .. }) => merge(d1, &singleton(d2)),
//         (Tangle::Dataflow { requires: req1, provides: prov1, nodes: n1, .. },
//          Tangle::Dataflow { requires: req2, provides: prov2, nodes: n2, .. }) => {
//             match (req1.intersection(prov2).next(), req2.intersection(prov1).next()) {
//                 (Some(_), Some(_)) => panic!("circular"),
//                 (None, Some(_)) => Tangle::Dataflow {
//                     name: d1.name() + d2.name(),
//                     requires: req1.union(req2.difference(prov1).into()),
//                     provides: prov1.union(prov2),
//                     nodes: n1.extend(iter)
//                 }
//                 (Some(_), None) =>
//                 (None, None) =>
//             }
//         }

//     }
// }

fn dedent(s: String, by: usize) -> String {
    let template = " ".repeat(by);
    match s.lines().next() {
        None => s,
        Some(first) => s
            .lines()
            .map(|l| {
                if l.starts_with(&template) {
                    l[by..].to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<String>>()
            .join("\r\n")
            .to_string(),
    }
}

fn remove_return(s: String) -> String {
    println!("remove ret {}", &s);
    match s.lines().next() {
        None => s,
        Some(first) => s
            .lines()
            .take_while(|l| !l.starts_with("return"))
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
            .join("\r\n"),
    }
}

pub fn topo_sort(flows: Vec<&Tangle>) -> Vec<Vec<String>> {
    let mut ts = topological_sort::TopologicalSort::<&String>::new();
    let mut inverse_provides = IndexMap::<&String, &String>::new();
    for node in &flows {
        let (_, provides) = node.deps();
        for p in provides {
            inverse_provides.insert(p, node.name());
        }
    }
    let mut ordering = vec![vec![]];
    let mut unseen: IndexSet<&String> = Default::default();
    for node in &flows {
        let (requires, provides) = node.deps();
        unseen.insert(node.name());
        for r in requires {
            if let Some(provider) = inverse_provides.get(r) {
                ts.add_dependency(*provider, node.name())
            }
        }
    }
    let ori_ordering = flows
        .iter()
        .map(|t| t.name())
        .enumerate()
        .map(|(i, n)| (n, i))
        .collect::<IndexMap<&String, usize>>();

    let seen: IndexSet<&String> = Default::default();
    while !ts.is_empty() {
        let mut level: Vec<String> = ts.pop_all().into_iter().map(|s| s.clone()).collect();
        level.sort_by_key(|f| ori_ordering[f]);
        for node in level.iter() {
            unseen.remove(node);
        }
        ordering.push(level);
    }

    if unseen.len() > 0 {
        ordering.push(unseen.into_iter().map(|s| s.clone()).collect());
    }

    ordering
}

pub type DocRange = (tree_sitter::Point, tree_sitter::Point);

pub fn identifier_occurs_before(
    ident: &IndexMap<String, DocRange>,
    id1: &String,
    id2: &String,
) -> Option<std::cmp::Ordering> {
    ident
        .get(id1)
        .zip(ident.get(id2))
        .map(|((p1, _), (p2, _))| p1.row < p2.row || ((p1.row == p2.row) && p1.column < p2.column))
        .map(|b| {
            if b {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        })
}

impl Tangle {
    pub fn from_leaves(name: String, nodes: IndexMap<String, Tangle>) -> Tangle {
        let ordering = topo_sort(nodes.values().collect());
        let all_requires: IndexSet<String> = ordering
            .iter()
            .flatten()
            .map(|n| nodes[n].deps().0.clone().into_iter())
            .flatten()
            .collect();
        let provides: IndexSet<String> = ordering
            .iter()
            .flatten()
            .map(|n| nodes[n].deps().1.clone().into_iter())
            .flatten()
            .collect();
        let requires = all_requires
            .difference(&provides)
            .map(|s| s.clone())
            .collect();

        Tangle::Dataflow {
            name,
            requires,
            provides,
            nodes,
        }
    }
    
    pub fn emit_all(&self) -> String {
        self.emit_decorated(&None)
    }
    pub fn emit_decorated(&self, decorate: &Option<&str>) -> String {
        match self {
            Tangle::Leaf { .. } => self.emit(),
            Tangle::Dataflow { nodes, .. } => {
                let leaves = nodes
                    .values()
                    .map(|t| {
                        if let Some(decorator) = decorate {
                            format!("{}(\"{}\")\r\n{}", decorator, t.name(), t.emit())
                        } else {
                            t.emit()
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("\r\n\r\n")
                    + "\r\n\r\n";
                leaves + self.emit().as_str()
            }
        }
    }

    // pub fn refine(&self) -> Self {
    //     match self {
    //         Tangle::Leaf { .. } => self.clone(),
    //         Tangle::Dataflow {
    //             nodes,
    //             requires,
    //             provides,
    //             name,
    //         } => {
    //             let used = nodes
    //                 .values()
    //                 .map(|n| n.deps().0)
    //                 .reduce(|req1, req2| &req1.union(req2).map(|s| s.clone()).collect());
    //             let new_nodes = if let Some(mut required) = used {
    //                 let mut new_nodes = nodes.clone();
    //                 for (_, node) in new_nodes.iter_mut() {
    //                     let node_provides = match node {
    //                         Tangle::Leaf { provides, .. } => provides,
    //                         Tangle::Dataflow { provides, .. } => provides,
    //                     };
    //                     *node_provides = node_provides
    //                         .intersection(required)
    //                         .map(|s| s.clone())
    //                         .collect();
    //                 }
    //                 new_nodes
    //             } else {
    //                 nodes.clone()
    //             };
    //             Self::Dataflow {
    //                 name: name.clone(),
    //                 requires: requires.clone(),
    //                 provides: provides.clone(),
    //                 nodes: new_nodes,
    //             }
    //         }
    //     }
    // }

    fn get_provides(
        code: &String,
    ) -> (
        IndexSet<String>,
        IndexMap<String, DocRange>,
        IndexSet<String>,
    ) {
        let ts = TangleString::from(code.as_str());
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language()).unwrap();
        let tree = parser.parse(code, None).unwrap();
        let root = tree.root_node();
        let captures = CELL_QUERY.capture_names();
        let mut modules: IndexSet<String> = Default::default();
        let mut identifier: IndexMap<String, DocRange> = Default::default();
        let mut node_provides: IndexSet<String> = Default::default();

        for qm in tree_sitter::QueryCursor::new().matches(&CELL_QUERY, tree.root_node(), ts) {
            for cap in qm.captures {
                match captures[cap.index as usize].as_str() {
                    "error" => {}
                    "import" => {
                        modules.insert(code[cap.node.byte_range()].to_string());
                        node_provides.insert(code[cap.node.byte_range()].to_string());
                    }
                    "identifier" => {
                        let start = cap.node.start_position();
                        if let Some(old) = identifier.insert(
                            code[cap.node.byte_range()].to_string(),
                            (cap.node.start_position(), cap.node.end_position()),
                        ) {
                            if old.0.row <= start.row && old.0.column <= start.column {
                                identifier.insert(code[cap.node.byte_range()].to_string(), old);
                            }
                        }
                    }
                    _ => {
                        node_provides.insert(code[cap.node.byte_range()].to_string());
                    }
                }
            }
        }
        // println!("{}", &root.to_sexp());

        (node_provides, identifier, modules)
    }

    fn get_requires(cell: &String) -> pyo3::PyResult<IndexSet<String>> {
        pyo3::Python::with_gil(|py| {
            let get_globals = py.import("tangle.pyapi")?.getattr("cell_load_globals")?;
            let builtins = py.import("builtins")?.dict();
            let cell_requires: Vec<String> = get_globals.call1((cell, builtins))?.extract()?;
            Ok(IndexSet::from_iter(cell_requires.into_iter()))
        })
    }

    pub fn from_cell(
        name: String,
        cell: &String,
    ) -> Result<
        (Tangle, IndexMap<String, DocRange>),
        (pyo3::PyErr, (Tangle, IndexMap<String, DocRange>)),
    > {
        let (provides, identifier, modules) = Tangle::get_provides(cell);
        match Tangle::get_requires(cell) {
            Ok(requires) => Ok((
                Tangle::Leaf {
                    code: cell.to_string(),
                    provides,
                    requires,
                    name,
                },
                identifier,
            )),
            Err(err) => Err((
                err,
                (
                    Tangle::Leaf {
                        code: cell.to_string(),
                        provides,
                        requires: Default::default(),
                        name,
                    },
                    identifier,
                ),
            )),
        }
    }

    fn flow_from_match(
        qm: &QueryMatch,
        leaves: &IndexMap<String, Tangle>,
        code: &String,
    ) -> Result<Tangle, Error> {
        let ts = TangleString::from(code.as_str());
        let get_code = |idx| {
            qm.nodes_for_capture_index(idx)
                .map(|n| code[n.byte_range()].to_string())
        };
        let name = get_code(*LEAF_NAME).into_iter().next().unwrap();
        let (requires, provides) = leaves[&name].deps();
        let mut uses_leaves: Vec<String> = vec![];
        for node in qm.nodes_for_capture_index(*FLOW_NODE) {
            for (qm, _) in tree_sitter::QueryCursor::new().captures(&NODE_QUERY, node, ts.clone()) {
                let get_code = |idx| {
                    qm.nodes_for_capture_index(idx)
                        .map(|n| code[n.byte_range()].to_string())
                };
                let node_name = get_code(*FLOW_NODE_NAME).next().unwrap();
                let node_requires = parse_id_or_seq(*FLOW_NODE_PARAM, &qm, &ts);
                let node_provides = parse_id_or_seq(*FLOW_NODE_PROVIDES, &qm, &ts);
                let leaf = leaves
                    .get(&node_name)
                    .ok_or_else(|| Error::MissingLeaf(node_name.clone()))?;
                if let Tangle::Leaf {
                    requires, provides, ..
                } = &leaf
                {
                    if !(requires == &node_requires) {
                        return Err(Error::RequiresMismatch {
                            expected: node_requires,
                            found: requires.clone(),
                        });
                    } else if !(provides == &node_provides) {
                        return Err(Error::ProvidesMismatch {
                            expected: node_provides,
                            found: provides.clone(),
                        });
                    }
                    uses_leaves.push(node_name);
                } else {
                    panic!("Only leaves for now")
                }
            }
        }
        Ok(Tangle::Dataflow {
            name,
            nodes: IndexMap::from_iter(
                uses_leaves
                    .into_iter()
                    .map(|n| (n.clone(), leaves.get(&n).map(|n| n.clone()).unwrap())),
            ),
            provides: provides.clone(),
            requires: requires.clone(),
        })
    }

    fn leaf_from_match(qm: &QueryMatch, code: &TangleString) -> Tangle {
        let get_code = |idx| {
            qm.nodes_for_capture_index(idx)
                .map(|n| code.0[n.byte_range()].to_string())
        };
        let name = get_code(*LEAF_NAME).into_iter().next();
        let requires = parse_id_or_seq(*LEAF_PARAM, &qm, &code);
        let provides = parse_id_or_seq(*LEAF_PROVIDES, &qm, &code);
        let indent = if let Some(node) = qm.nodes_for_capture_index(*LEAF_BODY).next() {
            node.start_position().column
        } else {
            0
        };
        let body = get_code(*LEAF_BODY)
            .into_iter()
            .map(|s| dedent(s, indent))
            .collect::<Vec<_>>()
            .join("\r\n");
        Tangle::Leaf {
            name: name.unwrap(),
            requires,
            provides,
            code: body,
        }
    }

    pub fn emit(&self) -> String {
        match self {
            Tangle::Leaf {
                name,
                provides,
                requires,
                code,
            } => {
                let requires = requires
                    .iter()
                    .map(|s| s.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                let head = format!("def {}({}):", name, requires);
                let body = code
                    .lines()
                    .map(|l| "    ".to_string() + l)
                    .collect::<Vec<_>>()
                    .join("\r\n");
                let provides = provides
                    .iter()
                    .map(|s| s.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("{}\r\n{}\r\n    return [{}]", head, body, provides)
            }
            Tangle::Dataflow {
                name,
                provides,
                requires,
                nodes,
            } => {
                let requires = requires
                    .iter()
                    .map(|s| s.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                let head = format!("def {}({}):", name, requires);
                let provides = provides
                    .iter()
                    .map(|s| s.clone())
                    .collect::<Vec<String>>()
                    .join(", ");
                let ordering = topo_sort(nodes.values().collect());
                let mut body: Vec<String> = vec![];
                for k in ordering.iter().flatten() {
                    let (requires, provides) = nodes.get(k).unwrap().deps();
                    let provides = provides
                        .iter()
                        .map(|s| s.clone())
                        .collect::<Vec<String>>()
                        .join(", ");
                    let requires = requires
                        .iter()
                        .map(|s| s.clone())
                        .collect::<Vec<String>>()
                        .join(", ");
                    body.push(format!("    [{}] = {}({})", provides, k, requires));
                }
                format!(
                    "{}\r\n{}\r\n    return [{}]",
                    head,
                    body.join("\r\n"),
                    provides
                )
            }
        }
    }

    pub fn from_code(code: String) -> Result<Tangle, Error> {
        let ts = TangleString::from(code.as_str());
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language()).unwrap();
        let tree = parser.parse(&code, None).unwrap();
        let root = tree.root_node();
        let sp = sexp::parse(root.to_sexp().as_str()).unwrap();
        println!("{:#?}", sp);
        let mut leaves: IndexMap<String, Tangle> = Default::default();
        let mut others: Vec<QueryMatch> = vec![];
        let mut cursor = tree_sitter::QueryCursor::new();
        for qm in cursor.matches(&QUERY, tree.root_node(), ts.clone()) {
            if qm.nodes_for_capture_index(*FLOW_NODE).next().is_none() {
                // leaf pattern
                let leaf = Tangle::leaf_from_match(&qm, &ts);
                leaves.insert(leaf.name().clone(), leaf);
            } else {
                let leaf = Tangle::leaf_from_match(&qm, &ts);
                leaves.insert(leaf.name().clone(), leaf);
                others.push(qm);
            }
        }

        for qm in others.into_iter() {
            // flow pattern
            match Tangle::flow_from_match(&qm, &leaves, &code) {
                Ok(flow) => {
                    if flow.name() == "dataflow" {
                        return Ok(flow);
                    } else {
                        dbg!(flow);
                    }
                }
                Err(why) => {
                    dbg!(why);
                }
            }
        }
        Err(Error::NoDataflowFound)
    }
}
