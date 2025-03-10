use super::State;
use crate::Config;
use std::{cell::RefCell, rc::Rc};
use swc_ecmascript::{
    ast::*,
    utils::{ident::IdentLike, ExprExt},
    visit::{as_folder, noop_visit_mut_type, noop_visit_type, Fold, Visit, VisitMut, VisitWith},
};

pub fn analyzer(config: Rc<Config>, state: Rc<RefCell<State>>) -> impl VisitMut + Fold {
    as_folder(AsAnalyzer { config, state })
}

struct AsAnalyzer {
    config: Rc<Config>,
    state: Rc<RefCell<State>>,
}

impl VisitMut for AsAnalyzer {
    noop_visit_mut_type!();

    fn visit_mut_module(&mut self, p: &mut Module) {
        let mut v = Analyzer {
            config: &self.config,
            state: &mut *self.state.borrow_mut(),
        };

        p.visit_with(&mut v);
    }

    fn visit_mut_script(&mut self, p: &mut Script) {
        let mut v = Analyzer {
            config: &self.config,
            state: &mut *self.state.borrow_mut(),
        };

        p.visit_with(&mut v);
    }
}

pub fn analyze(config: &Config, program: &Program) -> State {
    let mut state = State::default();

    let mut v = Analyzer {
        config,
        state: &mut state,
    };

    program.visit_with(&mut v);

    state
}

struct Analyzer<'a> {
    config: &'a Config,
    state: &'a mut State,
}

impl Visit for Analyzer<'_> {
    noop_visit_type!();

    fn visit_var_declarator(&mut self, v: &VarDeclarator) {
        v.visit_children_with(self);

        if let Pat::Ident(name) = &v.name {
            match v.init.as_deref() {
                Some(Expr::Call(CallExpr {
                    callee: ExprOrSuper::Expr(callee),
                    args,
                    ..
                })) => {
                    if callee.is_ident_ref_to("require".into())
                        && args.len() == 1
                        && args[0].spread.is_none()
                    {
                        match &*args[0].expr {
                            Expr::Lit(Lit::Str(v)) => {
                                let is_styled = if self.config.top_level_import_paths.is_empty() {
                                    &*v.value == "styled-components"
                                        || v.value.starts_with("styled-components/")
                                } else {
                                    self.config.top_level_import_paths.contains(&v.value)
                                };

                                if is_styled {
                                    self.state.styled_required = Some(name.id.to_id());
                                }
                            }
                            _ => {}
                        }
                    }
                }

                _ => {}
            }
        }
    }

    fn visit_import_decl(&mut self, i: &ImportDecl) {
        let is_custom = !self.config.top_level_import_paths.is_empty();

        let is_styled = if self.config.top_level_import_paths.is_empty() {
            &*i.src.value == "styled-components" || i.src.value.starts_with("styled-components/")
        } else {
            self.config.top_level_import_paths.contains(&i.src.value)
        };

        if is_styled {
            for s in &i.specifiers {
                match s {
                    ImportSpecifier::Named(s) => {
                        if is_custom
                            && s.imported
                                .as_ref()
                                .map(|v| &*v.sym)
                                .unwrap_or(&&*s.local.sym)
                                == "styled"
                        {
                            self.state.imported_local_name = Some(s.local.to_id());
                        }
                    }
                    ImportSpecifier::Default(s) => {
                        self.state.imported_local_name = Some(s.local.to_id());
                    }
                    ImportSpecifier::Namespace(s) => {
                        self.state.imported_local_ns = Some(s.local.to_id());
                    }
                }
            }
        }
    }
}
