use core::panic;
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::iter::FromIterator;
use std::mem::swap;
use std::ops::{Index, RangeBounds};
use std::sync::Arc;
use std::vec;

use eframe::epi::TextureAllocator;
use eframe::{egui, epi};
use egui::plot::Plot;
use egui::text::LayoutJob;
use egui::{CtxRef, Response, ScrollArea, Style, Ui};
use highlight::CodeTheme;
use indexmap::{IndexMap, IndexSet};
use tree_sitter::{Parser, Query, QueryCursor};

use pyo3::types::{IntoPyDict, PyDict, PyFunction, PyTuple};
use pyo3::{prelude::*, py_run};
use serde;
use std::rc::Rc;

#[macro_use]
extern crate lazy_static;
extern crate topological_sort;

mod highlight;
mod tangle;
mod text_buffer;

#[derive(Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct EditableNode {
    pub code: String,

    #[serde(skip)]
    pub leaf: tangle::Tangle,
    name: String,
    #[serde(skip)]
    last_output: Option<Py<PyAny>>,
    #[serde(skip)]
    highlighting: Option<LayoutJob>,
    #[serde(skip)]
    pub galley: Option<std::sync::Arc<egui::Galley>>,
    #[serde(skip)]
    pub text_off: egui::Vec2,
    #[serde(skip)]
    pub ident: IndexMap<String, tangle::DocRange>,
    #[serde(skip)]
    pub response: Option<egui::Response>,
    #[serde(skip)]
    pub debug_output_rect: Option<egui::Rect>,
    #[serde(skip)]
    pub changed: bool,
    pub changed_output: IndexSet<String>,
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct TangleApp {
    #[serde(with = "indexmap::serde_seq")]
    nodes: IndexMap<String, EditableNode>,
    focused: usize,
    style: Style,
    syntax_theme: CodeTheme,
    last_id: usize,
    bfs_layout: Option<Vec<Vec<String>>>,
    #[serde(skip)]
    curr_module: Option<Py<PyModule>>,
    #[serde(skip)]
    curr_flow: Option<tangle::Tangle>,
}

#[pyclass]
struct PyGUI {
    nodes: IndexMap<String, EditableNode>,
    ctx: CtxRef,
    frame: epi::Frame,
}

#[pyclass]
struct UiCtx {
    pub ui: safe_wrapper::SafeWrapper<Ui>,
    frame: epi::Frame,
}

#[pymethods]
impl PyGUI {
    fn window<'py>(&self, name: &str, show: &'py PyAny, py: Python<'py>) -> PyResult<&'py PyAny> {
        egui::Window::new(name)
            .show(&self.ctx, |ui| {
                safe_wrapper::SafeWrapper::scoped(py, ui, |ui_wrapper| {
                    let ctx = UiCtx {
                        ui: ui_wrapper,
                        frame: self.frame.clone(),
                    };
                    show.call1((ctx,))
                })
            })
            .unwrap()
            .inner
            .unwrap()
    }

    fn output_changed(&mut self, name: &str, changed_idx: usize) {
        dbg!(name, changed_idx);
        if let Some(node) = self.nodes.get_mut(name) {
            for (idx, provides) in node.leaf.deps().1.iter().enumerate() {
                if changed_idx == idx {
                    node.changed_output.insert(provides.clone());
                }
            }
            // node.changed_output = Some(node.leaf.deps().1.iter().enumerate().filter_map(|(i, name)| {
            //     if i == changed {
            //         Some(name.clone())
            //     } else {
            //         None
            //     }
            // }).collect());
        } else {
            panic!("no node with name {}", name);
        }
    }

    fn tangle_node_output<'py>(
        &mut self,
        name: &str,
        show: &'py PyAny,
        py: Python<'py>,
    ) -> PyResult<&'py PyAny> {
        let mut window = egui::Window::new(format!("output: {}", name));
        if let Some(debug_node_response) = &self.nodes.get(name).unwrap().response {
            window = window.current_pos(debug_node_response.rect.left_bottom());
        };

        let err = self.nodes.get(name).unwrap().last_output.is_some();

        // let mut frame = egui::Frame::group(&self.ctx.style());
        // if err {
        //     frame.fill = egui::Color32::RED;
        //     frame.shadow = egui::epaint::Shadow {
        //         extrusion: 100.0,
        //         color: egui::Color32::RED,
        //     };
        // }
        if let Some(window_response) = window.title_bar(false).show(&self.ctx, |ui| {
            safe_wrapper::SafeWrapper::scoped(py, ui, |ui_wrapper| {
                let ctx = UiCtx {
                    ui: ui_wrapper,
                    frame: self.frame.clone(),
                };
                show.call1((ctx,))
            })
        }) {
            self.nodes.get_mut(name).unwrap().debug_output_rect =
                Some(window_response.response.rect);
            match window_response.inner.unwrap() {
                Ok(ret) => Ok(ret),
                Err(exn) => {
                    self.nodes.get_mut(name).unwrap().last_output =
                        Some(exn.pvalue(py).into_py(py));
                    Err(exn)
                }
            }
        } else {
            show.call1((py.None().as_ref(py),))
        }
    }
}
use numpy::{PyArray1, PyArray2, PyArray3, PyReadonlyArray3};

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn selectable_text(ui: &mut egui::Ui, mut text: &str) {
    ui.add(egui::TextEdit::multiline(&mut text).desired_width(f32::INFINITY));
}

#[pymethods]
impl UiCtx {
    fn button(&mut self, label: &str, py: Python) -> PyResult<bool> {
        if let Some(ui) = self.ui.try_get_mut(py) {
            Ok(ui.button(label).is_pointer_button_down_on())
        } else {
            panic!("trying to use ui after liftime is over");
        }
    }

    fn slider(&mut self, label: &str, py: Python) -> PyResult<f32> {
        if let Some(ui) = self.ui.try_get_mut(py) {
            let id = egui::Id::new(label);
            let mut curr_val = 0.0;
            if let Some(val) = ui.memory().data.get_persisted::<f32>(id) {
                curr_val = val;
            }
            ui.add(egui::widgets::Slider::new(&mut curr_val, 0.0..=1.0));
            ui.memory().data.insert_persisted(id, curr_val);
            Ok(curr_val)
        } else {
            panic!("trying to use ui after liftime is over");
        }
    }

    fn image(&mut self, name: &str, img: PyReadonlyArray3<u8>, py: Python) -> PyResult<()> {
        if let Some(ui) = self.ui.try_get_mut(py) {
            let shape = img.shape();
            let (height, width, channels) = (shape[0], shape[1], shape[2]);
            // let flat = img;
            if channels == 4 {
                let slice = img.as_slice()?;
                let hash = calculate_hash(&slice);
                let tid = {
                    let mut mem = ui.memory();
                    let frame = self.frame.clone();
                    let (prev_hash, prev_id) =
                        mem.data
                            .get_temp_mut_or_insert_with(egui::Id::new(name), || {
                                let img =
                                    epi::Image::from_rgba_unmultiplied([width, height], slice);
                                (hash, frame.alloc(img))
                            });
                    if hash != *prev_hash {
                        self.frame.free(*prev_id);
                        let img = epi::Image::from_rgba_unmultiplied([width, height], slice);
                        *prev_id = self.frame.alloc(img);
                        *prev_hash = hash;
                        *prev_id
                    } else {
                        *prev_id
                    }
                };
                ui.image(tid, egui::vec2(height as f32, width as f32));
            }
            Ok(())
        } else {
            panic!("trying to use ui after liftime is over");
        }
    }

    fn visualize_py(&mut self, name: &str, val: &PyAny, py: Python) -> PyResult<()> {
        if let Some(ui) = self.ui.try_get_mut(py) {
            let np = py.import("numpy")?;
            let nf32 = np.getattr("float32")?;
            use numpy::PyArray1;

            if let Ok(Ok(dim)) = val.getattr("ndim").map(|v| v.extract::<i32>()) {
                if dim == 1 {
                    let arr: &PyArray1<f32> = val.getattr("astype")?.call1((nf32,))?.extract()?;
                    let readonly = arr.readonly();
                    let data = readonly.as_slice()?;
                    let line = egui::widgets::plot::Line::new(
                        egui::widgets::plot::Values::from_ys_f32(&data),
                    );
                    Plot::new(name).show(ui, |ui| ui.line(line));
                    Ok(())
                } else {
                    let repr = py.import("reprlib")?.getattr("repr")?;
                    let valrep: String = repr.call1((val,))?.extract()?;
                    ui.label(valrep);
                    Ok(())
                }
            } else if let Ok(exn) = val.downcast::<pyo3::exceptions::PyBaseException>() {
                ui.visuals_mut().widgets.noninteractive.bg_fill = egui::Color32::RED;
                let mut valrep: String = exn.repr()?.extract()?;
                ui.add(egui::widgets::TextEdit::multiline(&mut valrep).interactive(false));
                Ok(())
            } else {
                let repr = py.import("reprlib")?.getattr("repr")?;
                let valrep: String = repr.call1((val,))?.extract()?;
                ui.visuals_mut().widgets.noninteractive.bg_fill = egui::Color32::RED;
                ui.label(valrep);
                Ok(())
            }
        } else {
            panic!("trying to use ui after liftime is over");
        }
    }
}

trait WalkUi {
    fn ui(&self, ui: &mut egui::Ui, code: &str);
}

impl WalkUi for tree_sitter::Node<'_> {
    fn ui(&self, ui: &mut egui::Ui, code: &str) {
        ui.vertical(|ui| {
            ui.label(self.kind());
            ui.label(&code[self.start_byte()..self.end_byte()]);
            let mut cursor = self.walk();
            ui.horizontal(|ui| {
                self.children(&mut cursor).for_each(|n| n.ui(ui, code));
            })
        });
    }
}

impl PartialEq for EditableNode {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for EditableNode {}

impl EditableNode {
    fn new(name: &str, code: &str) -> Self {
        let mut last_output = None;
        let (leaf, ident) = match tangle::Tangle::from_cell(name.to_string(), &code.to_string()) {
            Ok((leaf, ident)) => (leaf, ident),
            Err((err, (partial_leaf, ident))) => {
                Python::with_gil(|py| last_output = Some(err.pvalue(py).into_py(py)));
                (partial_leaf, ident)
            }
        };
        Self {
            name: leaf.name().clone(),
            code: code.into(),
            leaf,
            last_output,
            highlighting: None,
            response: None,
            debug_output_rect: None,
            galley: None,
            text_off: egui::Vec2::ZERO,
            changed_output: Default::default(),
            ident,
            changed: false,
        }
    }

    fn get_sexp(code: &String) -> Option<String> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(tree_sitter_python::language());
        parser
            .parse(code, None)
            .map(|tree| tree.root_node().to_sexp())
    }

    fn visualize_py(
        ui: &mut egui::Ui,
        id: egui::Id,
        py: Python,
        val: &PyAny,
    ) -> PyResult<Response> {
        let np = py.import("numpy")?;
        let ndarray = np.getattr("ndarray")?;
        let nf32 = np.getattr("float32")?;
        use numpy::PyArray1;

        if let Ok(Ok(dim)) = val.getattr("ndim").map(|v| v.extract::<i32>()) {
            if dim == 1 {
                let arr: &PyArray1<f32> = val.getattr("astype")?.call1((nf32,))?.extract()?;
                let readonly = arr.readonly();
                let data = readonly.as_slice()?;
                let line =
                    egui::widgets::plot::Line::new(egui::widgets::plot::Values::from_ys_f32(&data));
                return Ok(Plot::new(id)
                    .view_aspect(1.0)
                    .data_aspect(1.0)
                    .show(ui, |ui| ui.line(line))
                    .response);
            }
        }
        let repr = py.import("reprlib")?.getattr("repr")?;
        let valrep: String = repr.call1((val,))?.extract()?;
        ui.visuals_mut().extreme_bg_color = egui::Color32::RED;
        Ok(ui.label(valrep))
    }

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        theme: &CodeTheme,
        exec: bool,
    ) -> (bool, std::sync::Arc<egui::Galley>) {
        let Self {
            name,
            code,
            last_output,
            highlighting,
            leaf,
            text_off,
            ident,
            ..
        } = self;
        let job = highlighting.get_or_insert_with(|| highlight::highlight(theme, code));
        if exec {
            ui.visuals_mut().window_shadow.color = egui::Color32::LIGHT_GREEN;
        }
        let (requires, provides) = leaf.deps();

        let mut req_label = requires.iter().map(|p| p.clone()).collect::<Vec<String>>();
        req_label.sort_unstable_by(|id1, id2| {
            tangle::identifier_occurs_before(ident, id1, id2).unwrap()
        });
        let mut prov_label = provides.iter().map(|p| p.clone()).collect::<Vec<String>>();
        prov_label.sort_unstable_by(|id1, id2| {
            tangle::identifier_occurs_before(ident, id1, id2).unwrap()
        });

        let resp = ui
            .horizontal(|ui| {
                // let close = ui
                //     .horizontal(|ui| {
                //         // let _resp = ui.label(if (provides.len() > 0) || (requires.len() > 0) {
                //         //     format!(
                //         //         "{}({}):{{{}}}",
                //         //         name.clone().as_str(),
                //         //         req_label.join(","),
                //         //         prov_label.join(",")
                //         //     )
                //         // } else {
                //         //     name.clone()
                //         // });
                //     })
                //     .inner;
                // ui.separator();
                let close = ui.button("⊗").clicked();
                *text_off = ui.min_rect().right_top().to_vec2() + egui::Vec2 { x: 10.0, y: 5.0 };
                (
                    close,
                    crate::highlight::code_view_ui(ui, code, theme, exec, job),
                )
            })
            .inner;

        if let Some(prev) = last_output {
            Python::with_gil(|py| {
                let pformat = py.import("pprint").unwrap().getattr("pformat").unwrap();
                // pformat.call1((prev.as_ref(py),)).unwrap().str()
                let repr = pformat.call1((prev.as_ref(py),)).unwrap().str().unwrap();
                let pretty = format!("{}", repr.to_str().unwrap());
                ui.separator();
                ui.label(pretty);
            });
        }

        (resp.0, resp.1.galley)
    }
}

impl TangleApp {
    fn bfs_ui(&self) -> Vec<Vec<String>> {
        tangle::topo_sort(self.nodes.values().map(|n| &n.leaf).collect())
    }

    fn fresh_node(&mut self) {
        while let Some(_) = self.nodes.get(&format!("node_{}", self.last_id)) {
            self.last_id += 1;
        }
        let id = format!("node_{}", self.last_id);

        self.nodes.insert(
            format!("node_{}", self.last_id),
            EditableNode::new(&format!("node_{}", self.last_id), ""),
        );
        // if let Some(layout) = &mut self.bfs_layout {
        //     layout.push(vec![id.clone()]);
        // }
        self.bfs_layout = Some(self.bfs_ui());
        self.last_id += 1;
    }

    fn compile(&self) -> tangle::Tangle {
        tangle::Tangle::from_leaves(
            "dataflow".to_string(),
            self.nodes
                .values()
                .map(|n| (n.name.clone(), n.leaf.clone()))
                .collect(),
        )
    }

    fn compile_cells(&mut self) {
        for (name, node) in self.nodes.iter_mut() {
            if let tangle::Tangle::Leaf { code, .. } = &node.leaf {
                if &node.code != code {
                    match tangle::Tangle::from_cell(name.clone(), &node.code) {
                        Err((exn, (partial_leaf, ident))) => Python::with_gil(|py| {
                            node.last_output = Some(exn.pvalue(py).into_py(py))
                        }),
                        Ok((leaf, ident)) => {
                            node.ident = ident;
                            node.code = leaf.code().clone();
                            node.leaf = leaf;
                            node.last_output = None;
                        }
                    }
                }
            } else {
                panic!();
            }
        }
    }

    fn compile_fresh_flow(&self) -> (tangle::Tangle, PyResult<Py<PyModule>>) {
        let flow = self.compile();
        let code = flow.emit_decorated(&Some("@tangle.pyapi.memo"));
        let full_module = format!("import tangle.pyapi\r\n\r\nprint('hey')\r\n\r\n{}", code);
        (
            flow,
            Python::with_gil(|py| {
                pyo3::types::PyModule::from_code(py, full_module.as_str(), "<dataflow>", "dataflow")
                    .map(|m| m.into_py(py))
            }),
        )
    }

    fn compile_module(&mut self) -> PyResult<()> {
        self.compile_cells();
        let (flow, new_module) = self.compile_fresh_flow();
        self.curr_module = Some(new_module?);
        self.curr_flow = Some(flow);
        self.bfs_layout = Some(self.bfs_ui());
        Ok(())
    }

    fn run_module(&mut self, ctx: &CtxRef, frame: epi::Frame) -> PyResult<()> {
        let Self {
            curr_module,
            curr_flow,
            nodes,
            ..
        } = self;
        if let Some(flow_mod_py) = curr_module {
            Python::with_gil(|py| {
                let flow = curr_flow.as_ref().unwrap();
                let (requires, provides) = flow.deps();
                let flow_mod = flow_mod_py.as_ref(py);
                let pyflow = flow_mod.getattr("dataflow")?;
                let pyapi = py.import("tangle.pyapi")?;
                let set_gui_ref = pyapi.getattr("set_gui_ref")?;
                let ui_wrapper = pyapi.getattr("UiWrapper")?;
                let gui = PyCell::new(
                    py,
                    PyGUI {
                        ctx: ctx.clone(),
                        nodes: nodes.clone(),
                        frame: frame.clone(),
                    },
                )
                .unwrap();
                set_gui_ref.call1((gui,)).unwrap();
                let wrapped = ui_wrapper.call0()?;

                // println!("{}", flow.emit_decorated(&Some("@tangle.pyapi.memo")));
                let ret = if requires.contains("gui") {
                    pyflow.call((), Some(vec![("gui", wrapped)].into_py_dict(py)))
                } else {
                    pyflow.call0()
                }?;
                swap(&mut gui.borrow_mut().nodes, nodes);
                PyResult::Ok(())
            })
        } else {
            Ok(())
        }
    }
}

impl epi::App for TangleApp {
    fn name(&self) -> &str {
        "tangle"
    }

    fn clear_color(&self) -> egui::Rgba {
        egui::Rgba::from_black_alpha(0.0)
    }

    fn setup(
        &mut self,
        ctx: &egui::CtxRef,
        _frame: &epi::Frame,
        storage: Option<&dyn epi::Storage>,
    ) {
        if let Some(store) = storage {
            if let Some(me) = epi::get_value(store, "me") {
                *self = me;
            }
            if let Some(theme) = epi::get_value(store, "theme") {
                self.syntax_theme = theme;
            }
        }

        let tst = include_str!("test_flow.py");
        let tangle = tangle::Tangle::from_code(tst.to_string());
        if let Ok(tangle) = tangle {
            // println!("{}", &tangle.emit_all());
            self.nodes.clear();
            self.bfs_layout.take();
            match tangle {
                tangle::Tangle::Dataflow {
                    nodes,
                    requires,
                    provides,
                    ..
                } => {
                    self.nodes = nodes
                        .into_iter()
                        .map(|(name, t)| {
                            (
                                name.clone(),
                                EditableNode::new(name.as_str(), t.code().as_str()),
                            )
                        })
                        .collect();
                    self.bfs_layout = Some(self.bfs_ui());
                }
                _ => {}
            }
        } else {
            panic!();
        }

        ctx.set_style(self.style.clone());
        ctx.set_visuals(self.style.visuals.clone());
        ctx.set_pixels_per_point(0.5);
    }

    fn save(&mut self, storage: &mut dyn epi::Storage) {
        epi::set_value(storage, "me", self);
        epi::set_value(storage, "theme", &self.syntax_theme);
    }

    fn update(&mut self, ctx: &egui::CtxRef, frame: &epi::Frame) {
        let mut exec = false;
        // self.syntax_theme = CodeTheme::default();
        for event in &ctx.input().events {
            match event {
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers: egui::Modifiers { shift: true, .. },
                } => {
                    egui::Window::new("exec").show(ctx, |ui| {
                        match self.compile_module() {
                            Err(exn) => exn.print(Python::acquire_gil().python()),
                            Ok(_) => {}
                        };
                        exec = true;
                    });
                }
                _ => {}
            }
        }

        let mut remove_at = None;
        let theme = &self.syntax_theme.clone();
        let mut any_empty = false;
        if let Some(layout) = &self.bfs_layout {
            let mut row_off = egui::Pos2::ZERO + egui::Vec2::new(120.0, 120.0);
            for row in layout {
                let mut col_off = egui::Rect::from_min_max(row_off, row_off);
                for col in row {
                    if let Some(node) = self.nodes.get_mut(col) {
                        let window = egui::Window::new(col)
                            .title_bar(false)
                            .current_pos(col_off.right_top())
                            .show(ctx, |ui| node.ui(ui, theme, exec))
                            .unwrap();

                        let (close, galley) = window.inner.unwrap();
                        col_off = col_off.union(window.response.rect);
                        col_off.extend_with_x(col_off.right() + 14.0);
                        if let Some(dbg_rect) = node.debug_output_rect {
                            col_off.extend_with_y(dbg_rect.bottom());
                        }
                        if close {
                            remove_at = Some(col.clone());
                        }
                        any_empty |= node.code.len() == 0;
                        node.changed_output.clear();
                        node.response = Some(window.response);
                        node.galley = Some(galley);
                    } else {
                        dbg!(row, col);
                        panic!();
                    }
                }
                row_off.y += col_off.height() + 14.0;
            }
        } else {
            self.bfs_layout = Some(self.bfs_ui());
        }

        if let Some(module) = &self.curr_module {
            if let Err(exn) = self.run_module(ctx, frame.clone()) {
                exn.print(Python::acquire_gil().python());
            };

            egui::Window::new("dbg emit").show(ctx, |ui| {
                let mut code = self
                    .curr_flow
                    .as_ref()
                    .unwrap()
                    .emit_decorated(&Some("@tangle.pyapi.memo"));
                ui.add(egui::widgets::TextEdit::multiline(&mut code).interactive(false));
            });
        }

        if exec && !any_empty {
            self.fresh_node();
        } else if exec {
            self.bfs_layout = Some(self.bfs_ui());
        }
        if let Some(at) = remove_at {
            self.nodes.remove(&at);
            self.bfs_layout = Some(self.bfs_ui());
        }

        egui::Window::new("theme edit").show(ctx, |ui| {
            ctx.style_ui(ui);
            self.style = (*ctx.style()).clone();
        });

        egui::Window::new("syntax theme edit").show(ctx, |ui| {
            if self.syntax_theme.ui(ui) {
                self.nodes.values_mut().for_each(|n| {
                    n.highlighting.take();
                })
            }
        });

        if let Some(layout) = &self.bfs_layout {
            let painter = ctx.layer_painter(egui::LayerId::background());
            for level in layout {
                for node in level.iter().filter_map(|n| self.nodes.get(n)) {
                    let (requires, provides) = node.leaf.deps();
                    for (dep_var, dep, pt, endpt) in provides.iter().flat_map(|dep_var| {
                        self.nodes.iter().filter_map(move |(_, dep_node)| {
                            if dep_node.leaf.deps().0.contains(dep_var) {
                                Some((
                                    dep_var,
                                    dep_node,
                                    node.ident.get(dep_var).unwrap(),
                                    dep_node.ident.get(dep_var).unwrap(),
                                ))
                            } else {
                                None
                            }
                        })
                    }) {
                        let mut soff = 0.0;
                        let mut loff = 0.0;
                        let time = std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .unwrap();
                        let s = time.as_secs_f64() % 10.0;
                        let colour: egui::Color32 = {
                            soff = ctx.animate_bool(
                                egui::Id::new(&node.name).with(dep_var),
                                node.changed_output.contains(dep_var),
                            );
                            soff = soff.sqrt();
                            let off = egui::Rgba::from(egui::Color32::LIGHT_GREEN);
                            let on = egui::Rgba::from(egui::Color32::LIGHT_BLUE);
                            off * (1.0 - soff) + (on * soff)
                        }
                        .into();
                        if let Some((g1, g2)) = node.galley.clone().zip(dep.galley.clone()) {
                            let start = (egui::vec2(
                                g1.rows[pt.0.row].x_offset(pt.0.column),
                                g1.rows[pt.0.row].max_y(),
                            ) + egui::vec2(
                                g1.rows[pt.1.row].x_offset(pt.1.column),
                                g1.rows[pt.1.row].max_y(),
                            )) / 2.0;
                            let end = (egui::vec2(
                                g2.rows[endpt.0.row].x_offset(endpt.0.column),
                                g2.rows[endpt.0.row].max_y(),
                            ) + egui::vec2(
                                g2.rows[endpt.1.row].x_offset(endpt.1.column),
                                g2.rows[endpt.1.row].max_y(),
                            )) / 2.0;
                            let to = (dep.text_off + end) - (node.text_off + start);
                            for i in (0..100) {
                                let t = (i as f32) / 100.0;
                                let tn = ((i + 1) as f32) / 100.0;
                                let toff = (time.subsec_millis() as f32) / 1000.0;
                                let w1 = ((t - toff) * 50.0).sin();
                                let w2 = ((tn - toff) * 50.0).sin();
                                let w = (w1 + w2) * soff;
                                let t2 = t;
                                let tn2 = tn;
                                let off = to * egui::Vec2 { x: t2, y: t };
                                let off2 = to * egui::Vec2 { x: tn2, y: tn };
                                painter.line_segment(
                                    [
                                        (node.text_off + start).to_pos2() + off,
                                        (node.text_off + start).to_pos2() + off2,
                                    ],
                                    egui::Stroke::new(2.0 + 2.0 * w, colour.additive()),
                                );
                            }
                            // painter.line_segment(
                            //     [
                            //         node.text_off.to_pos2() + start,
                            //         dep.text_off.to_pos2() + end,
                            //     ],
                            //     egui::Stroke::new(2.0, colour),
                            // );
                        }
                    }
                }
            }
        } else {
            self.bfs_layout = Some(self.bfs_ui());
        }
        // self.nodes.clear();
        // self.bfs_layout = None;

        if self.nodes.len() == 0 {
            self.fresh_node();
            self.last_id = 1;
        }

        // self.nodes.clear()
    }
}

fn gui() {
    let app = TangleApp {
        nodes: IndexMap::new(),
        style: egui::Style::default(),
        syntax_theme: CodeTheme::default(),
        focused: 0,
        last_id: 0,
        bfs_layout: None,
        curr_module: None,
        curr_flow: None,
    };
    let mut native_options = eframe::NativeOptions::default();
    native_options.transparent = true;
    // native_options.decorated = false;
    eframe::run_native(Box::new(app), native_options);
}
/// A Python module implemented in Rust.
#[pymodule]
fn tangle(_py: Python, _m: &PyModule) -> PyResult<()> {
    // pyo3::prepare_freethreaded_python();
    // println!("{:?}", pyapi.getattr("compile_extract").call1());
    gui();
    Ok(())
}

// from https://gist.github.com/raphlinus/479df97a7ba715b494b87af9071e9b87
mod safe_wrapper {
    use pyo3::Python;
    use std::sync::atomic::{AtomicPtr, Ordering};

    pub struct SafeWrapper<T>(*mut AtomicPtr<T>);

    unsafe impl<T> Send for SafeWrapper<T> {}

    impl<T> Drop for SafeWrapper<T> {
        fn drop(&mut self) {
            unsafe {
                let box_ptr = self.0;
                let ptr = &*(box_ptr as *const AtomicPtr<String>);
                let old = ptr.load(Ordering::Acquire);
                ptr.store(std::ptr::null_mut(), Ordering::Release);
                if old.is_null() {
                    std::mem::drop(Box::from_raw(box_ptr));
                }
            }
        }
    }

    impl<T> SafeWrapper<T> {
        pub fn scoped<'p, U>(
            _py: Python<'p>,
            obj: &mut T,
            f: impl FnOnce(SafeWrapper<T>) -> U,
        ) -> U {
            let box_ptr = Box::into_raw(Box::new(AtomicPtr::new(obj)));
            let wrapper = SafeWrapper(box_ptr);
            let result = f(wrapper);
            std::mem::drop(SafeWrapper(box_ptr));
            result
        }

        pub fn try_get_mut<'p>(&mut self, _py: Python<'p>) -> Option<&mut T> {
            unsafe {
                let ptr = (*self.0).load(Ordering::Relaxed);
                if ptr.is_null() {
                    None
                } else {
                    Some(&mut *ptr)
                }
            }
        }
    }
}
