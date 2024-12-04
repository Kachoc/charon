use crate::ids::Vector;
use crate::{ast::*, common::hash_consing::HashConsed};
use derive_visitor::{Drive, DriveMut, Event, Visitor, VisitorMut};
use macros::{EnumAsGetters, EnumIsA, EnumToGetters, VariantIndexArity, VariantName};
use serde::{Deserialize, Serialize};

pub type FieldName = String;

// We need to manipulate a lot of indices for the types, variables, definitions,
// etc. In order not to confuse them, we define an index type for every one of
// them (which is just a struct with a unique usize field), together with some
// utilities like a fresh index generator. Those structures and utilities are
// generated by using macros.
generate_index_type!(TypeVarId, "T");
generate_index_type!(VariantId, "Variant");
generate_index_type!(FieldId, "Field");
generate_index_type!(RegionId, "Region");
generate_index_type!(ConstGenericVarId, "Const");

/// Type variable.
/// We make sure not to mix variables and type variables by having two distinct
/// definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub struct TypeVar {
    /// Unique index identifying the variable
    pub index: TypeVarId,
    /// Variable name
    pub name: String,
}

/// Region variable.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord, Drive, DriveMut,
)]
pub struct RegionVar {
    /// Unique index identifying the variable
    pub index: RegionId,
    /// Region name
    pub name: Option<String>,
}

/// Const Generic Variable
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub struct ConstGenericVar {
    /// Unique index identifying the variable
    pub index: ConstGenericVarId,
    /// Const generic name
    pub name: String,
    /// Type of the const generic
    pub ty: LiteralTy,
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Copy,
    Clone,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
)]
#[serde(transparent)]
pub struct DeBruijnId {
    pub index: usize,
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Copy,
    Clone,
    Hash,
    PartialOrd,
    Ord,
    EnumIsA,
    EnumAsGetters,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
)]
#[charon::variants_prefix("R")]
pub enum Region {
    /// Static region
    Static,
    /// Bound region variable.
    ///
    /// **Important**:
    /// ==============
    /// Similarly to what the Rust compiler does, we use De Bruijn indices to
    /// identify *groups* of bound variables, and variable identifiers to
    /// identity the variables inside the groups.
    ///
    /// For instance, we have the following:
    /// ```text
    ///                     we compute the De Bruijn indices from here
    ///                            VVVVVVVVVVVVVVVVVVVVVVV
    /// fn f<'a, 'b>(x: for<'c> fn(&'a u8, &'b u16, &'c u32) -> u64) {}
    ///      ^^^^^^         ^^       ^       ^        ^
    ///        |      De Bruijn: 0   |       |        |
    ///  De Bruijn: 1                |       |        |
    ///                        De Bruijn: 1  |    De Bruijn: 0
    ///                           Var id: 0  |       Var id: 0
    ///                                      |
    ///                                De Bruijn: 1
    ///                                   Var id: 1
    /// ```
    BVar(DeBruijnId, RegionId),
    /// Erased region
    Erased,
    /// For error reporting.
    #[charon::opaque]
    Unknown,
}

/// Identifier of a trait instance.
/// This is derived from the trait resolution.
///
/// Should be read as a path inside the trait clauses which apply to the current
/// definition. Note that every path designated by [TraitInstanceId] refers
/// to a *trait instance*, which is why the [Clause] variant may seem redundant
/// with some of the other variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Drive, DriveMut)]
#[charon::rename("TraitInstanceId")]
pub enum TraitRefKind {
    /// A specific top-level implementation item.
    TraitImpl(TraitImplId, GenericArgs),

    /// One of the local clauses.
    ///
    /// Example:
    /// ```text
    /// fn f<T>(...) where T : Foo
    ///                    ^^^^^^^
    ///                    Clause(0)
    /// ```
    Clause(TraitClauseId),

    /// A parent clause
    ///
    /// Remark: the [TraitDeclId] gives the trait declaration which is
    /// implemented by the instance id from which we take the parent clause
    /// (see example below). It is not necessary and included for convenience.
    ///
    /// Remark: Ideally we should store a full `TraitRef` instead, but hax does not give us enough
    /// information to get the right generic args.
    ///
    /// Example:
    /// ```text
    /// trait Foo1 {}
    /// trait Foo2 { fn f(); }
    ///
    /// trait Bar : Foo1 + Foo2 {}
    ///             ^^^^   ^^^^
    ///                    parent clause 1
    ///     parent clause 0
    ///
    /// fn g<T : Bar>(x : T) {
    ///   x.f()
    ///   ^^^^^
    ///   Parent(Clause(0), Bar, 1)::f(x)
    ///                          ^
    ///                          parent clause 1 of clause 0
    ///                     ^^^
    ///              clause 0 implements Bar
    /// }
    /// ```
    ParentClause(Box<TraitRefKind>, TraitDeclId, TraitClauseId),

    /// A clause defined on an associated type. This variant is only used during translation; after
    /// the `lift_associated_item_clauses` pass, clauses on items become `ParentClause`s.
    ///
    /// Remark: the [TraitDeclId] gives the trait declaration which is
    /// implemented by the trait implementation from which we take the item
    /// (see below). It is not necessary and provided for convenience.
    ///
    /// Example:
    /// ```text
    /// trait Foo {
    ///   type W: Bar0 + Bar1 // Bar1 contains a method bar1
    ///                  ^^^^
    ///               this is the clause 1 applying to W
    /// }
    ///
    /// fn f<T : Foo>(x : T::W) {
    ///   x.bar1();
    ///   ^^^^^^^
    ///   ItemClause(Clause(0), Foo, W, 1)
    ///                              ^^^^
    ///                              clause 1 from item W (from local clause 0)
    ///                         ^^^
    ///                local clause 0 implements Foo
    /// }
    /// ```
    #[charon::opaque]
    ItemClause(Box<TraitRefKind>, TraitDeclId, TraitItemName, TraitClauseId),

    /// Self, in case of trait declarations/implementations.
    ///
    /// Putting [Self] at the end on purpose, so that when ordering the clauses
    /// we start with the other clauses (in particular, the local clauses). It
    /// is useful to give priority to the local clauses when solving the trait
    /// obligations which are fullfilled by the trait parameters.
    #[charon::rename("Self")]
    SelfId,

    /// A specific builtin trait implementation like [core::marker::Sized] or
    /// auto trait implementation like [core::marker::Syn].
    BuiltinOrAuto(PolyTraitDeclRef),

    /// The automatically-generated implementation for `dyn Trait`.
    Dyn(PolyTraitDeclRef),

    /// For error reporting.
    #[charon::rename("UnknownTrait")]
    Unknown(String),
}

/// A reference to a trait
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Drive, DriveMut)]
pub struct TraitRef {
    #[charon::rename("trait_id")]
    pub kind: TraitRefKind,
    /// Not necessary, but useful
    pub trait_decl_ref: PolyTraitDeclRef,
}

/// A predicate of the form `Type: Trait<Args>`.
///
/// About the generics, if we write:
/// ```text
/// impl Foo<bool> for String { ... }
/// ```
///
/// The substitution is: `[String, bool]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Drive, DriveMut)]
pub struct TraitDeclRef {
    #[charon::rename("trait_decl_id")]
    pub trait_id: TraitDeclId,
    #[charon::rename("decl_generics")]
    pub generics: GenericArgs,
}

/// A quantified trait predicate, e.g. `for<'a> Type<'a>: Trait<'a, Args>`.
pub type PolyTraitDeclRef = RegionBinder<TraitDeclRef>;

/// A reference to a tait impl, using the provided arguments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Drive, DriveMut)]
pub struct TraitImplRef {
    #[charon::rename("trait_impl_id")]
    pub impl_id: TraitImplId,
    #[charon::rename("impl_generics")]
    pub generics: GenericArgs,
}

/// .0 outlives .1
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutlivesPred<T, U>(pub T, pub U);

// The derive macro doesn't handle generics well.
impl<T: Drive, U: Drive> Drive for OutlivesPred<T, U> {
    fn drive<V: Visitor>(&self, visitor: &mut V) {
        visitor.visit(self, Event::Enter);
        self.0.drive(visitor);
        self.1.drive(visitor);
        visitor.visit(self, Event::Exit);
    }
}
impl<T: DriveMut, U: DriveMut> DriveMut for OutlivesPred<T, U> {
    fn drive_mut<V: VisitorMut>(&mut self, visitor: &mut V) {
        visitor.visit(self, Event::Enter);
        self.0.drive_mut(visitor);
        self.1.drive_mut(visitor);
        visitor.visit(self, Event::Exit);
    }
}

pub type RegionOutlives = OutlivesPred<Region, Region>;
pub type TypeOutlives = OutlivesPred<Ty, Region>;

/// A constraint over a trait associated type.
///
/// Example:
/// ```text
/// T : Foo<S = String>
///         ^^^^^^^^^^
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Drive, DriveMut)]
pub struct TraitTypeConstraint {
    pub trait_ref: TraitRef,
    pub type_name: TraitItemName,
    pub ty: Ty,
}

#[derive(Default, Clone, Eq, PartialEq, Serialize, Deserialize, Hash, Drive, DriveMut)]
pub struct GenericArgs {
    pub regions: Vector<RegionId, Region>,
    pub types: Vector<TypeVarId, Ty>,
    pub const_generics: Vector<ConstGenericVarId, ConstGeneric>,
    // TODO: rename to match [GenericParams]?
    pub trait_refs: Vector<TraitClauseId, TraitRef>,
}

/// A value of type `T` bound by generic parameters. Used in any context where we're adding generic
/// parameters that aren't on the top-level item, e.g. `for<'a>` clauses, trait methods (TODO),
/// GATs (TODO).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RegionBinder<T> {
    #[charon::rename("binder_regions")]
    pub regions: Vector<RegionId, RegionVar>,
    /// Named this way to highlight accesses to the inner value that might be handling parameters
    /// incorrectly. Prefer using helper methods.
    #[charon::rename("binder_value")]
    pub skip_binder: T,
}

/// Generic parameters for a declaration.
/// We group the generics which come from the Rust compiler substitutions
/// (the regions, types and const generics) as well as the trait clauses.
/// The reason is that we consider that those are parameters that need to
/// be filled. We group in a different place the predicates which are not
/// trait clauses, because those enforce constraints but do not need to
/// be filled with witnesses/instances.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub struct GenericParams {
    pub regions: Vector<RegionId, RegionVar>,
    pub types: Vector<TypeVarId, TypeVar>,
    pub const_generics: Vector<ConstGenericVarId, ConstGenericVar>,
    // TODO: rename to match [GenericArgs]?
    pub trait_clauses: Vector<TraitClauseId, TraitClause>,
    /// The first region in the pair outlives the second region
    pub regions_outlive: Vec<RegionBinder<RegionOutlives>>,
    /// The type outlives the region
    pub types_outlive: Vec<RegionBinder<TypeOutlives>>,
    /// Constraints over trait associated types
    pub trait_type_constraints: Vec<RegionBinder<TraitTypeConstraint>>,
}

/// A predicate of the form `exists<T> where T: Trait`.
///
/// TODO: store something useful here
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub struct ExistentialPredicate;

generate_index_type!(TraitClauseId, "TraitClause");

/// A predicate of the form `Type: Trait<Args>`.
#[derive(Debug, Clone, Serialize, Deserialize, Drive, DriveMut)]
pub struct TraitClause {
    /// We use this id when solving trait constraints, to be able to refer
    /// to specific where clauses when the selected trait actually is linked
    /// to a parameter.
    pub clause_id: TraitClauseId,
    // TODO: does not need to be an option.
    pub span: Option<Span>,
    /// Where the predicate was written, relative to the item that requires it.
    #[charon::opaque]
    pub origin: PredicateOrigin,
    /// The trait that is implemented.
    #[charon::rename("trait")]
    pub trait_: PolyTraitDeclRef,
}

impl PartialEq for TraitClause {
    fn eq(&self, other: &Self) -> bool {
        // Skip `span` and `origin`
        self.clause_id == other.clause_id && self.trait_ == other.trait_
    }
}

impl Eq for TraitClause {}

/// Where a given predicate came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub enum PredicateOrigin {
    // Note: we use this for globals too, but that's only available with an unstable feature.
    // ```
    // fn function<T: Clone>() {}
    // fn function<T>() where T: Clone {}
    // const NONE<T: Copy>: Option<T> = None;
    // ```
    WhereClauseOnFn,
    // ```
    // struct Struct<T: Clone> {}
    // struct Struct<T> where T: Clone {}
    // type TypeAlias<T: Clone> = ...;
    // ```
    WhereClauseOnType,
    // Note: this is both trait impls and inherent impl blocks.
    // ```
    // impl<T: Clone> Type<T> {}
    // impl<T> Type<T> where T: Clone {}
    // impl<T> Trait for Type<T> where T: Clone {}
    // ```
    WhereClauseOnImpl,
    // The special `Self: Trait` clause which is in scope inside the definition of `Foo` or an
    // implementation of it.
    // ```
    // trait Trait {}
    // ```
    TraitSelf,
    // Note: this also includes supertrait constraings.
    // ```
    // trait Trait<T: Clone> {}
    // trait Trait<T> where T: Clone {}
    // trait Trait: Clone {}
    // ```
    WhereClauseOnTrait,
    // ```
    // trait Trait {
    //     type AssocType: Clone;
    // }
    // ```
    TraitItem(TraitItemName),
}

/// A type declaration.
///
/// Types can be opaque or transparent.
///
/// Transparent types are local types not marked as opaque.
/// Opaque types are the others: local types marked as opaque, and non-local
/// types (coming from external dependencies).
///
/// In case the type is transparent, the declaration also contains the
/// type definition (see [TypeDeclKind]).
///
/// A type can only be an ADT (structure or enumeration), as type aliases are
/// inlined in MIR.
#[derive(Debug, Clone, Serialize, Deserialize, Drive, DriveMut)]
pub struct TypeDecl {
    #[drive(skip)]
    pub def_id: TypeDeclId,
    /// Meta information associated with the item.
    pub item_meta: ItemMeta,
    pub generics: GenericParams,
    /// The type kind: enum, struct, or opaque.
    pub kind: TypeDeclKind,
}

#[derive(Debug, Clone, EnumIsA, EnumAsGetters, Serialize, Deserialize, Drive, DriveMut)]
pub enum TypeDeclKind {
    Struct(Vector<FieldId, Field>),
    Enum(Vector<VariantId, Variant>),
    Union(Vector<FieldId, Field>),
    /// An opaque type.
    ///
    /// Either a local type marked as opaque, or an external type.
    Opaque,
    /// An alias to another type. This only shows up in the top-level list of items, as rustc
    /// inlines uses of type aliases everywhere else.
    Alias(Ty),
    /// Used if an error happened during the extraction, and we don't panic
    /// on error.
    #[charon::rename("TError")]
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Drive, DriveMut)]
pub struct Variant {
    pub span: Span,
    pub attr_info: AttrInfo,
    #[charon::rename("variant_name")]
    pub name: String,
    pub fields: Vector<FieldId, Field>,
    /// The discriminant used at runtime. This is used in `remove_read_discriminant` to match up
    /// `SwitchInt` targets with the corresponding `Variant`.
    pub discriminant: ScalarValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, Drive, DriveMut)]
pub struct Field {
    pub span: Span,
    pub attr_info: AttrInfo,
    #[charon::rename("field_name")]
    pub name: Option<String>,
    #[charon::rename("field_ty")]
    pub ty: Ty,
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Copy,
    Clone,
    EnumIsA,
    VariantName,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
    Hash,
    Ord,
    PartialOrd,
)]
#[charon::rename("IntegerType")]
pub enum IntegerTy {
    Isize,
    I8,
    I16,
    I32,
    I64,
    I128,
    Usize,
    U8,
    U16,
    U32,
    U64,
    U128,
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Copy,
    Clone,
    EnumIsA,
    VariantName,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
    Hash,
    Ord,
    PartialOrd,
)]
#[charon::rename("FloatType")]
pub enum FloatTy {
    F16,
    F32,
    F64,
    F128,
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Hash,
    VariantName,
    EnumIsA,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
    Ord,
    PartialOrd,
)]
#[charon::variants_prefix("R")]
pub enum RefKind {
    Mut,
    Shared,
}

/// Type identifier.
///
/// Allows us to factorize the code for built-in types, adts and tuples
#[derive(
    Debug,
    PartialEq,
    Eq,
    Clone,
    Copy,
    VariantName,
    EnumAsGetters,
    EnumIsA,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
    Hash,
    Ord,
    PartialOrd,
)]
#[charon::variants_prefix("T")]
pub enum TypeId {
    /// A "regular" ADT type.
    ///
    /// Includes transparent ADTs and opaque ADTs (local ADTs marked as opaque,
    /// and external ADTs).
    #[charon::rename("TAdtId")]
    Adt(TypeDeclId),
    Tuple,
    /// Built-in type. Either a primitive type like array or slice, or a
    /// non-primitive type coming from a standard library
    /// and that we handle like a primitive type. Types falling into this
    /// category include: Box, Vec, Cell...
    /// The Array and Slice types were initially modelled as primitive in
    /// the [Ty] type. We decided to move them to built-in types as it allows
    /// for more uniform treatment throughout the codebase.
    #[charon::rename("TBuiltin")]
    Builtin(BuiltinTy),
}

/// Types of primitive values. Either an integer, bool, char
#[derive(
    Debug,
    PartialEq,
    Eq,
    Clone,
    Copy,
    VariantName,
    EnumIsA,
    EnumAsGetters,
    VariantIndexArity,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
    Hash,
    Ord,
    PartialOrd,
)]
#[charon::rename("LiteralType")]
#[charon::variants_prefix("T")]
pub enum LiteralTy {
    Integer(IntegerTy),
    Float(FloatTy),
    Bool,
    Char,
}

/// Const Generic Values. Either a primitive value, or a variable corresponding to a primitve value
#[derive(
    Debug,
    PartialEq,
    Eq,
    Clone,
    VariantName,
    EnumIsA,
    EnumAsGetters,
    VariantIndexArity,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
    Hash,
)]
#[charon::variants_prefix("Cg")]
pub enum ConstGeneric {
    /// A global constant
    Global(GlobalDeclId),
    /// A const generic variable
    Var(ConstGenericVarId),
    /// A concrete value
    Value(Literal),
}

/// A type.
///
/// Warning: for performance reasons, the `Drive` and `DriveMut` impls of `Ty` don't explore the
/// contents of the type, they only yield a pointer to the type itself. To recurse into the type,
/// use `drive_inner{_mut}` or `visit_inside`.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ty(HashConsed<TyKind>);

impl Ty {
    pub fn new(kind: TyKind) -> Self {
        Ty(HashConsed::new(kind))
    }

    pub fn kind(&self) -> &TyKind {
        self.0.inner()
    }

    pub fn drive_inner<V: Visitor>(&self, visitor: &mut V) {
        self.0.drive(visitor)
    }
    pub fn drive_inner_mut<V: VisitorMut>(&mut self, visitor: &mut V) {
        self.0.drive_mut(visitor)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    VariantName,
    EnumIsA,
    EnumAsGetters,
    EnumToGetters,
    VariantIndexArity,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
)]
#[charon::variants_prefix("T")]
#[charon::rename("Ty")]
pub enum TyKind {
    /// An ADT.
    /// Note that here ADTs are very general. They can be:
    /// - user-defined ADTs
    /// - tuples (including `unit`, which is a 0-tuple)
    /// - built-in types (includes some primitive types, e.g., arrays or slices)
    /// The information on the nature of the ADT is stored in (`TypeId`)[TypeId].
    /// The last list is used encode const generics, e.g., the size of an array
    ///
    /// Note: this is incorrectly named: this can refer to any valid `TypeDecl` including extern
    /// types.
    Adt(TypeId, GenericArgs),
    #[charon::rename("TVar")]
    TypeVar(TypeVarId),
    Literal(LiteralTy),
    /// The never type, for computations which don't return. It is sometimes
    /// necessary for intermediate variables. For instance, if we do (coming
    /// from the rust documentation):
    /// ```text
    /// let num: u32 = match get_a_number() {
    ///     Some(num) => num,
    ///     None => break,
    /// };
    /// ```
    /// the second branch will have type `Never`. Also note that `Never`
    /// can be coerced to any type.
    ///
    /// Note that we eliminate the variables which have this type in a micro-pass.
    /// As statements don't have types, this type disappears eventually disappears
    /// from the AST.
    Never,
    // We don't support floating point numbers on purpose (for now)
    /// A borrow
    Ref(Region, Ty, RefKind),
    /// A raw pointer.
    RawPtr(Ty, RefKind),
    /// A trait associated type
    ///
    /// Ex.:
    /// ```text
    /// trait Foo {
    ///   type Bar; // type associated to the trait Foo
    /// }
    /// ```
    TraitType(TraitRef, TraitItemName),
    /// `dyn Trait`
    ///
    /// This carries an existentially quantified list of predicates, e.g. `exists<T> where T:
    /// Into<u64>`. The predicate must quantify over a single type and no any regions or constants.
    ///
    /// TODO: we don't translate this properly yet.
    DynTrait(ExistentialPredicate),
    /// Arrow type, used in particular for the local function pointers.
    /// This is essentially a "constrained" function signature:
    /// arrow types can only contain generic lifetime parameters
    /// (no generic types), no predicates, etc.
    Arrow(RegionBinder<(Vec<Ty>, Ty)>),
}

/// Builtin types identifiers.
///
/// WARNING: for now, all the built-in types are covariant in the generic
/// parameters (if there are). Adding types which don't satisfy this
/// will require to update the code abstracting the signatures (to properly
/// take into account the lifetime constraints).
///
/// TODO: update to not hardcode the types (except `Box` maybe) and be more
/// modular.
/// TODO: move to builtins.rs?
#[derive(
    Debug,
    PartialEq,
    Eq,
    Clone,
    Copy,
    EnumIsA,
    EnumAsGetters,
    VariantName,
    Serialize,
    Deserialize,
    Drive,
    DriveMut,
    Hash,
    Ord,
    PartialOrd,
)]
#[charon::variants_prefix("T")]
pub enum BuiltinTy {
    /// Boxes are de facto a primitive type.
    Box,
    /// Primitive type
    Array,
    /// Primitive type
    Slice,
    /// Primitive type
    Str,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub enum ClosureKind {
    Fn,
    FnMut,
    FnOnce,
}

/// Additional information for closures.
/// We mostly use it in micro-passes like [crate::update_closure_signature].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub struct ClosureInfo {
    pub kind: ClosureKind,
    /// Contains the types of the fields in the closure state.
    /// More precisely, for every place captured by the
    /// closure, the state has one field (typically a ref).
    ///
    /// For instance, below the closure has a state with two fields of type `&u32`:
    /// ```text
    /// pub fn test_closure_capture(x: u32, y: u32) -> u32 {
    ///   let f = &|z| x + y + z;
    ///   (f)(0)
    /// }
    /// ```
    pub state: Vector<TypeVarId, Ty>,
}

/// A function signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Drive, DriveMut)]
pub struct FunSig {
    /// Is the function unsafe or not
    pub is_unsafe: bool,
    /// `true` if the signature is for a closure.
    ///
    /// Importantly: if the signature is for a closure, then:
    /// - the type and const generic params actually come from the parent function
    ///   (the function in which the closure is defined)
    /// - the region variables are local to the closure
    pub is_closure: bool,
    /// Additional information if this is the signature of a closure.
    pub closure_info: Option<ClosureInfo>,
    pub generics: GenericParams,
    pub inputs: Vec<Ty>,
    pub output: Ty,
}
