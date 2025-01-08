//! We have added an explicit `Self: Trait` clause to the function and global items that correspond
//! to a trait method/associated const declaration. This pass removes the clause in question if it
//! is not used by the item.
use derive_generic_visitor::*;
use std::collections::HashSet;

use crate::ast::*;

use super::{ctx::TransformPass, TransformCtx};

struct FoundClause;

struct UsesClauseVisitor(TraitClauseId);
impl Visitor for UsesClauseVisitor {
    type Break = FoundClause;
}

/// Visit an item looking for uses of the given clause.
impl VisitAst for UsesClauseVisitor {
    fn visit_trait_clause_id(&mut self, x: &TraitClauseId) -> ControlFlow<Self::Break> {
        if *x == self.0 {
            Break(FoundClause)
        } else {
            Continue(())
        }
    }
    fn visit_trait_clause(&mut self, _: &TraitClause) -> ControlFlow<Self::Break> {
        // Don't look inside the clause declaration as this will always contain the
        // `TraitClauseId`.
        Continue(())
    }
    fn visit_fun_decl(&mut self, x: &FunDecl) -> ControlFlow<Self::Break> {
        if let Err(Opaque) = x.body {
            // For function without bodies we can't know whether the clause is used so we err on
            // the side of it being used.
            return Break(FoundClause);
        }
        self.visit_inner(x)
    }
}

pub struct Transform;
impl TransformPass for Transform {
    fn transform_ctx(&self, ctx: &mut TransformCtx) {
        let self_clause_id = TraitClauseId::from_raw(0);
        let mut doesnt_use_self: HashSet<AnyTransId> = Default::default();

        // We explore only items with an explicit `Self` clause, namely method and associated const
        // declarations.
        for tdecl in &ctx.translated.trait_decls {
            let methods = tdecl.methods().map(|(_, bound_fn)| bound_fn.skip_binder.id);
            // For consts, we need to explore the corresponding initializer body.
            let consts = tdecl
                .const_defaults
                .iter()
                .filter_map(|(_, x)| ctx.translated.global_decls.get(x.id))
                .map(|gdecl| gdecl.init);
            let funs = methods
                .chain(consts)
                .filter_map(|id: FunDeclId| ctx.translated.fun_decls.get(id));
            for fun in funs {
                match fun.drive(&mut UsesClauseVisitor(self_clause_id)) {
                    Continue(()) => {
                        doesnt_use_self.insert(fun.def_id.into());
                        if let Some(gid) = fun.is_global_initializer {
                            doesnt_use_self.insert(gid.into());
                        }
                    }
                    Break(FoundClause) => {}
                }
            }
        }

        // In each item, remove the first clause and renumber the others.
        for &id in &doesnt_use_self {
            let Some(mut item) = ctx.translated.get_item_mut(id) else {
                continue;
            };
            item.generic_params()
                .trait_clauses
                .remove_and_shift_ids(self_clause_id);
            item.dyn_visit_mut(|clause_id: &mut TraitClauseId| {
                *clause_id = TraitClauseId::from_usize(clause_id.index() - 1);
            });
        }

        // Update any `GenericArgs` destined for the items we just changed.
        ctx.translated.dyn_visit_mut(|args: &mut GenericArgs| {
            if let GenericsSource::Item(target_id) = args.target
                && doesnt_use_self.contains(&target_id)
            {
                args.trait_refs.remove_and_shift_ids(self_clause_id);
            }
        });
    }
}
