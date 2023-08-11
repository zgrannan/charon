//! Defines some utilities for [crate::names]
//!
//! For now, we have one function per object kind (type, trait, function,
//! module): many of them could be factorized (will do).
#![allow(dead_code)]

use crate::names::*;
use hax_frontend_exporter as hax;
use hax_frontend_exporter::SInto;
use rustc_hir::{Item, ItemKind};
use rustc_middle::ty::TyCtxt;
use serde::{Serialize, Serializer};
use std::collections::HashSet;

impl PathElem {
    // TODO: we could make that an eq trait?
    // On the other hand I'm not fond of overloading...
    fn equals_ident(&self, id: &str) -> bool {
        match self {
            PathElem::Ident(s) => s == id,
            PathElem::Disambiguator(_) => false,
        }
    }
}

impl std::fmt::Display for PathElem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        match self {
            PathElem::Ident(s) => {
                write!(f, "{s}")
            }
            PathElem::Disambiguator(d) => {
                write!(f, "{d}")
            }
        }
    }
}

impl Name {
    pub fn from(name: Vec<String>) -> Name {
        Name {
            name: name.into_iter().map(PathElem::Ident).collect(),
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.name.len()
    }

    /// Compare the name to a constant array.
    /// This ignores disambiguators.
    ///
    /// `equal`: if `true`, check that the name is equal to the ref. If `false`:
    /// only check if the ref is a prefix of the name.
    pub fn compare_with_ref_name(&self, equal: bool, ref_name: &[&str]) -> bool {
        let name: Vec<&PathElem> = self.name.iter().filter(|e| e.is_ident()).collect();

        if name.len() < ref_name.len() || (equal && name.len() != ref_name.len()) {
            return false;
        }

        for i in 0..ref_name.len() {
            if !name[i].equals_ident(ref_name[i]) {
                return false;
            }
        }
        true
    }

    /// Compare the name to a constant array.
    /// This ignores disambiguators.
    pub fn equals_ref_name(&self, ref_name: &[&str]) -> bool {
        self.compare_with_ref_name(true, ref_name)
    }

    /// Compare the name to a constant array.
    /// This ignores disambiguators.
    pub fn prefix_is_same(&self, ref_name: &[&str]) -> bool {
        self.compare_with_ref_name(false, ref_name)
    }

    /// Return `true` if the name identifies an item inside the module: `krate::module`
    pub fn is_in_module(&self, krate: &String, module: &String) -> bool {
        self.prefix_is_same(&[krate, module])
    }

    /// Similar to [Name::is_in_module]
    pub fn is_in_modules(&self, krate: &String, modules: &HashSet<String>) -> bool {
        if self.len() >= 2 {
            match (&self.name[0], &self.name[1]) {
                (PathElem::Ident(s0), PathElem::Ident(s1)) => s0 == krate && modules.contains(s1),
                _ => false,
            }
        } else {
            false
        }
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        let v: Vec<String> = self.name.iter().map(|s| s.to_string()).collect();
        write!(f, "{}", v.join("::"))
    }
}

impl Serialize for Name {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use crate::common::*;
        let name = VecSerializer::new(&self.name);
        name.serialize(serializer)
    }
}

/// Retrieve an item name from a [DefId].
pub fn def_id_to_name(def_id: &hax::DefId) -> ItemName {
    trace!("{:?}", def_id);

    // We have to be a bit careful when retrieving names from def ids. For instance,
    // due to reexports, [`TyCtxt::def_path_str`](TyCtxt::def_path_str) might give
    // different names depending on the def id on which it is called, even though
    // those def ids might actually identify the same definition.
    // For instance: `std::boxed::Box` and `alloc::boxed::Box` are actually
    // the same (the first one is a reexport).
    // This is why we implement a custom function to retrieve the original name
    // (though this makes us loose aliases - we may want to investigate this
    // issue in the future).

    // We lookup the path associated to an id, and convert it to a name.
    // Paths very precisely identify where an item is. There are important
    // subcases, like the items in an `Impl` block:
    // ```
    // impl<T> List<T> {
    //   fn new() ...
    // }
    // ```
    //
    // One issue here is that "List" *doesn't appear* in the path, which would
    // look like the following:
    //
    //   `TypeNS("Crate") :: Impl :: ValueNs("new")`
    //                       ^^^
    //           This is where "List" should be
    //
    // For this reason, whenever we find an `Impl` path element, we actually
    // lookup the type of the sub-path, which allows us to retrieve
    // the type identifier. We then grab its last path element of the type
    // identifier (say the identifier is "list::List", we only use "List")
    // and insert it in the name.
    //
    // Besides, as there may be several "impl" blocks for one type, each impl
    // block is identified by a unique number (rustc calls this a
    // "disambiguator"), which we grab.
    //
    // Example:
    // ========
    // For instance, if we write the following code in crate `test` and module
    // `bla`:
    // ```
    // impl<T> Foo<T> {
    //   fn foo() { ... }
    // }
    //
    // impl<T> Foo<T> {
    //   fn bar() { ... }
    // }
    // ```
    //
    // The names we will generate for `foo` and `bar` are:
    // `[Ident("test"), Ident("bla"), Ident("Foo"), Disambiguator(0), Ident("foo")]`
    // `[Ident("test"), Ident("bla"), Ident("Foo"), Disambiguator(1), Ident("bar")]`
    let mut found_crate_name = false;
    let mut id = def_id;
    let mut name: Vec<PathElem> = Vec::new();

    // Rk.: below we try to be as tight as possible with regards to sanity
    // checks, to make sure we understand what happens with def paths, and
    // fail whenever we get something which is even slightly outside what
    // we expect.
    for data in &id.path {
        // Match over the key data
        use hax::DefPathItem;
        match &data.data {
            DefPathItem::TypeNs(symbol) => {
                assert!(data.disambiguator == 0); // Sanity check
                name.push(PathElem::Ident(symbol.clone()));
            }
            DefPathItem::ValueNs(symbol) => {
                if data.disambiguator != 0 {
                    // I don't like that

                    // I think this only happens with names introduced by macros
                    // (though not sure). For instance:
                    // `betree_main::betree_utils::_#1::{impl#0}::deserialize::{impl#0}`
                    let s = symbol;
                    assert!(s == "_");
                    name.push(PathElem::Ident(s.clone()));
                    name.push(PathElem::Disambiguator(Disambiguator::Id::new(
                        data.disambiguator as usize,
                    )));
                } else {
                    name.push(PathElem::Ident(symbol.clone()));
                }
            }
            DefPathItem::CrateRoot => {
                // Sanity check
                assert!(data.disambiguator == 0);

                // This should be the beginning of the path
                assert!(name.is_empty());
                found_crate_name = true;
                name.push(PathElem::Ident(def_id.krate.clone()));
            }
            DefPathItem::Impl { ty, .. } => {
                // Match over the type.
                use hax::Ty;
                name.push(PathElem::Ident(match ty {
                    Ty::Adt { def_id: adt_id, .. } => {
                        let mut type_name = def_id_to_name(adt_id);
                        type_name.name.pop().unwrap().to_string()
                    }
                    // Builtin cases.
                    Ty::Int(_) | Ty::Uint(_) | Ty::Array(..) | Ty::Slice(_) => {
                        format!("{ty:?}")
                    }
                    _ => unreachable!(),
                }));

                // Push the disambiguator
                name.push(PathElem::Disambiguator(Disambiguator::Id::new(
                    data.disambiguator as usize,
                )));
            }
            DefPathItem::ImplTrait => {
                // TODO: this should work the same as for `Impl`
                unimplemented!();
            }
            DefPathItem::MacroNs(symbol) => {
                assert!(data.disambiguator == 0); // Sanity check

                // There may be namespace collisions between, say, function
                // names and macros (not sure). However, this isn't much
                // of an issue here, because for now we don't expose macros
                // in the AST, and only use macro names in [register], for
                // instance to filter opaque modules.
                name.push(PathElem::Ident(symbol.clone()));
            }
            _ => {
                error!("Unexpected DefPathItem: {:?}", data);
                unreachable!("Unexpected DefPathItem: {:?}", data);
            }
        }
    }

    // We always add the crate name
    if !found_crate_name {
        name.push(PathElem::Ident(def_id.krate.clone()));
    }

    // Reverse the name and return
    name.reverse();
    trace!("{:?}", name);
    Name { name }
}

/// Returns an optional name for an HIR item.
///
/// If the option is `None`, it means the item is to be ignored (example: it
/// is a `use` item).
///
/// Rk.: this function is only used by [crate::register], and implemented with this
/// context in mind.
pub fn hir_item_to_name(tcx: TyCtxt, item: &Item) -> Option<HirItemName> {
    // TODO: factor this out
    let state = hax::state::State::new(
        tcx,
        &hax::options::Options {
            inline_macro_calls: Vec::new(),
        },
    );
    let def_id = item.owner_id.to_def_id().sinto(&state);

    match &item.kind {
        ItemKind::OpaqueTy(_) => unimplemented!(),
        ItemKind::Union(_, _) => unimplemented!(),
        ItemKind::ExternCrate(_) => {
            // We ignore this -
            // TODO: investigate when extern crates appear, and why
            Option::None
        }
        ItemKind::Use(_, _) => Option::None,
        ItemKind::TyAlias(_, _) => {
            // We ignore the type aliases - it seems they are inlined
            Option::None
        }
        ItemKind::Enum(_, _)
        | ItemKind::Struct(_, _)
        | ItemKind::Fn(_, _, _)
        | ItemKind::Impl(_)
        | ItemKind::Mod(_)
        | ItemKind::Const(_, _)
        | ItemKind::Static(_, _, _)
        | ItemKind::Macro(_, _) => Option::Some(def_id_to_name(&def_id)),
        _ => {
            unimplemented!("{:?}", item.kind);
        }
    }
}

// TODO: remove
pub fn item_def_id_to_name(tcx: TyCtxt, def_id: rustc_span::def_id::DefId) -> ItemName {
    // TODO: factor this out
    let state = hax::state::State::new(
        tcx,
        &hax::options::Options {
            inline_macro_calls: Vec::new(),
        },
    );
    def_id_to_name(&def_id.sinto(&state))
}
