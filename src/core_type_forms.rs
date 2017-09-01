/*
 * The type theory for Unseemly
 *  is largely swiped from the "Types and Programming Languages" by Pierce.
 * I've agressively copied the formally-elegant but non-ergonomic theory
 *  whenever I think that the ergonomic way of doing things is just syntax sugar over it.
 * After all, syntax sugar is the point of Unseemly!
 *
 * I didn't think that I could survive making a system out of + and × types, though,
 *  so there are n-ary `struct`s and `enum`s.
 */

 /*
There are two similar things we should distinguish!
(1) syntax for types, as written by the user in an `Ast`
(2) types themselves, the result of type synthesis, often stored in `Ty`
     (which is just a thin wrapper around `Ast`).

These things are almost identical,
 which is why postive synth_type is usually implemented with `LiteralLike`.

We should also distinguish
(3) ___, (normally also called "types"). The ___ of an expression is a type,
     and the ___ of a type is a kind.


It is at this point that I am reminded of a passage from GEB:

 Now in set theory, which deals with abstractions that we don't use all the time, a
 stratification like the theory of types seems acceptable, even if a little strange-but when it
 comes to language, an all-pervading part of life, such stratification appears absurd. We
 don't think of ourselves as jumping up and down a hierarchy of languages when we speak
 about various things. A rather matter-of-fact sentence such as, "In this book, I criticize
 the theory of types" would be doubly forbidden in the system we are discussing. Firstly, it
 mentions "this book", which should only be mentionable in a metabook-and secondly, it mentions
 me-a person whom I should not be allowed to speak of at all! This example points out how silly
 the theory of types seems, when you import it into a familiar context. The remedy it adopts for
 paradoxes-total banishment of self-reference in any form-is a real case of overkill, branding
 many perfectly good constructions as meaningless. The adjective "meaningless", by the way,
 would have to apply to all discussions of the theory of linguistic types (such as that of this
 very paragraph) for they clearly could not occur on any of the levels-neither object language,
 nor metalanguage, nor metametalanguage, etc. So the very act of discussing the theory
 would be the most blatant possible violation of it!

   — Douglas Hofstadter, Godel, Escher, Bach: and Eternal Golden Braid

*/

use std::rc::Rc;
use parse::{SynEnv, FormPat};
use form::{Form, simple_form, BiDiWR, Positive, Negative, Both};
use parse::FormPat::*;
use ast_walk::{WalkRule, WalkMode, walk, WalkElt, NegativeWalkMode};
use ast_walk::WalkRule::*;
use name::*;
use core_forms::ast_to_name;
use ty::{Ty, synth_type, UnpackTy, TyErr, SynthTy};
use ty_compare::{Canonicalize, Subtype};
use ast::*;
use ::util::assoc::Assoc;

//TODO: I think we need to extend `Form` with `synth_kind`...
fn type_defn(form_name: &str, p: FormPat) -> Rc<Form> {
    Rc::new(Form {
        name: n(form_name),
        grammar: Rc::new(p),
        type_compare: Both(LiteralLike, LiteralLike),
        synth_type: Positive(LiteralLike),
        quasiquote: Both(LiteralLike, LiteralLike),
        eval: Positive(NotWalked)
    })
}

fn type_defn_complex(form_name: &str, p: FormPat, sy: WalkRule<SynthTy>,
                     tc: BiDiWR<Canonicalize, Subtype>) -> Rc<Form> {
    Rc::new(Form {
        name: n(form_name),
        grammar: Rc::new(p),
        type_compare: tc,
        synth_type: Positive(sy),
        quasiquote: Both(LiteralLike, LiteralLike),
        eval: Positive(NotWalked)
    })
}

pub fn make_core_syn_env_types() -> SynEnv {
    /* Regarding the value/type/kind hierarchy, Benjamin Pierce generously assures us that
        "For programming languages ... three levels have proved sufficient." */

    /* kinds */
    let _type_kind = simple_form("type", form_pat!((lit "*")));
    let _higher_kind = simple_form("higher", form_pat!(
        (delim "k[", "[", /*]]*/
            [ (star (named "param", (call "kind"))), (lit "->"), (named "res", (call "kind"))])));


    /* types */
    let fn_type =
        type_defn_complex("fn",
            /* Friggin' Atom bracket matching doesn't ignore strings or comments. */
            form_pat!((delim "[", "[", /*]]*/
                [ (star (named "param", (call "type"))), (lit "->"),
                  (named "ret", (call "type") ) ])),
            LiteralLike, // synth is normal
            Both(LiteralLike,
                cust_rc_box!(move |fn_parts| {
                    let actual = fn_parts.context_elt().concrete();
                    let actual_parts = try!(Subtype::context_match(
                        &fn_parts.this_ast, &actual, fn_parts.env.clone()));

                    let expd_params = fn_parts.get_rep_term(&n("param"));
                    let actl_params = actual_parts.get_rep_leaf_or_panic(&n("param"));
                    if expd_params.len() != actl_params.len() {
                        return Err(TyErr::LengthMismatch(
                            actl_params.iter().map(|&a| Ty(a.clone())).collect(),
                            expd_params.len()));
                    }
                    for (p_expected, p_got) in expd_params.iter().zip(actl_params.iter()) {
                        // Parameters have reversed subtyping:
                        let _ : ::util::assoc::Assoc<Name, Ty> = try!(walk::<Subtype>(
                            *p_got, &fn_parts.with_context(Ty::new(p_expected.clone()))));
                    }

                    walk::<Subtype>(&fn_parts.get_term(&n("ret")),
                        &fn_parts.with_context(
                            Ty::new(actual_parts.get_leaf_or_panic(&n("ret")).clone())))
                 }))
        );

    let enum_type =
        type_defn("enum", form_pat!([(lit "enum"),
            (delim "{", "{", /*}}*/ (star [(named "name", aat),
                (delim "(", "(", /*))*/ (star (named "component", (call "type"))))]))]));

    let struct_type =
        type_defn("struct", form_pat!(
            [(lit "struct"),
             (delim "{", "{", /*}}*/ (star [(named "component_name", aat), (lit ":"),
                                            (named "component", (call "type"))]))]));

    let forall_type =
        type_defn_complex("forall_type",
            form_pat!([(lit "forall"), (star (named "param", aat)), (lit "."),
                       (named "body", (import [* [forall "param"]], (call "type")))]),
            LiteralLike, // synth is normal
            Both(
                LiteralLike,
                cust_rc_box!(move |forall_parts| {
                    match Subtype::context_match(
                            &forall_parts.this_ast,
                            &forall_parts.context_elt().concrete(),
                            forall_parts.env.clone()) {
                        // ∀ X. ⋯ <: ∀ Y. ⋯ ? (so force X=Y)
                        Ok(actual_forall_parts) => {
                            let actl_inner_body =
                                actual_forall_parts.get_leaf_or_panic(&n("body"));

                            walk::<Subtype>(&forall_parts.get_term(&n("body")),
                                &forall_parts.with_context(Ty::new(actl_inner_body.clone())))
                        }
                        // ∀ X. ⋯ <: ⋯ ?  (so try to specialize X)
                        Err(_) => {
                            // `import [forall "param"]` handles the specialization,
                            //  and we leave the context element alone
                            walk::<Subtype>(&forall_parts.get_term(&n("body")), &forall_parts)
                        }
                    }
                })));

    /* This behaves slightly differently than the `mu` from Pierce's book,
     *  because we need to support mutual recursion.
     * In particular, it relies on having a binding for `param` in the environment!
     * The only thing that `mu` actually does is suppress substitution,
     *  to prevent the attempted generation of an infinite type.
     */
    let mu_type = type_defn_complex("mu_type",
        form_pat!([(lit "mu_type"), (star (named "param", aat)), (lit "."),
             (named "body", (import [* [prot "param"]], (call "type")))]),
        LiteralLike,
        Both(
            LiteralLike,
            cust_rc_box!(move |mu_parts| {
                let rhs_mu_parts = try!(Subtype::context_match(
                    &mu_parts.this_ast,
                    &mu_parts.context_elt().concrete(),
                    mu_parts.env.clone()));

                let rhs_body = rhs_mu_parts.get_leaf_or_panic(&n("body"));

                let r_params = rhs_mu_parts.get_rep_leaf_or_panic(&n("param"));
                let l_params = mu_parts.get_rep_term(&n("param"));
                if r_params.len() != l_params.len() {
                    return Err(TyErr::LengthMismatch(
                        r_params.iter().map(|a| Ty((*a).clone())).collect(), l_params.len()));
                }
                // Apply the Amber rule; assume the `mu`ed names are subtypes to subtype the bodies
                let mut amber_environment = mu_parts.env.clone();
                for (&p_r, p_l) in r_params.iter().zip(l_params.iter()) {
                    if p_r == p_l // short-circuit if the names are the same...
                        || mu_parts.env.find(&ast_to_name(p_r)) // ...or Amber assumed so already
                             == Some(&Ty(VariableReference(ast_to_name(&p_l)))) { continue; }

                    // print!("Ambering: {} = {}\n", p_r, p_l);
                    amber_environment = amber_environment.set(
                        ast_to_name(p_r), Ty(VariableReference(::core_forms::ast_to_name(p_l))));
                }

                walk::<Subtype>(&mu_parts.get_term(&n("body")),
                    &mu_parts.with_environment(amber_environment)
                        .with_context(Ty::new(rhs_body.clone())))
            })));


    // This only makes sense inside a concrete syntax type or during typechecking.
    // For example, the type of the `let` macro is (where `dotdotdot_type` is `...[]...`):
    // ∀ ...{T}... . ∀ S .
    //    '[let ...[ ,[ var ⇑ v ], = ,[ expr<[T]< ], ]...
    //            in ,[ expr<[S]< ↓ ...{v = T}...], ]'
    //        -> expr<[S]<
    // TODO: add named repeats. Add type-level numbers!
    let dotdotdot_type = type_defn("dotdotdot",
        form_pat!((delim "...[", "[", /*]]*/ (named "body", (call "type")))));

    // Like a variable reference (but `LiteralLike` typing prevents us from doing that)
    // TODO: I think this can be removed, and replaced with `VariableReference` now
    let type_by_name = type_defn_complex("type_by_name",
        form_pat!([(lit "DEPRECATED"), (named "name", aat)]),
        cust_rc_box!(move |tbn_part| {
            let name = ast_to_name(&tbn_part.get_term(&n("name")));
            ::ty::SynthTy::walk_var(name, &tbn_part)
        }),
        Both(
            cust_rc_box!(move |tbn_part| {
                ::ty_compare::Canonicalize::walk_var(
                    ast_to_name(&tbn_part.get_term(&n("name"))), &tbn_part)
            }),
            cust_rc_box!(move |tbn_part| {
                ::ty_compare::Subtype::walk_var(
                    ast_to_name(&tbn_part.get_term(&n("name"))), &tbn_part)

            })));

    let forall_type_0 = forall_type.clone();

   /* [Type theory alert!]
    * Pierce's notion of type application is an expression, not a type;
    *  you just take an expression whose type is a `forall`, and then give it some arguments.
    * Instead, we will just make the type system unify `forall` types with more specific types.
    * But sometimes the user wants to write a more specific type, and they use this.
    *
    * This is, at the type level, like function application.
    * We restrict the LHS to being a name, because that's "normal". Should we?
    */
    let type_apply = type_defn_complex("type_apply",
        // The technical term for `<[...]<` is "fish X-ray"
        form_pat!([(named "type_name", aat),
         (delim "<[", "[", /*]]*/ (star [(named "arg", (call "type"))]))]),
        cust_rc_box!(move |tapp_parts| {
            let arg_res = try!(tapp_parts.get_rep_res(&n("arg")));

            let type_name = ast_to_name(&tapp_parts.get_term(&n("type_name")));

            match tapp_parts.env.find(&type_name) {
                None => ty_err!(UnboundName(type_name) at tapp_parts.this_ast),
                Some(&Ty(VariableReference(same))) if same == type_name => {
                    // e.g. `X<[int, Y]<` underneath `mu X. ...`

                    // Rebuild a type_by_name, but evaulate its arguments
                    // This kind of thing is necessary because
                    //  we wish to avoid aliasing problems at the type level.
                    // In System F, this is avoided by performing capture-avoiding substitution.
                    let mut new__tapp_parts = ::util::mbe::EnvMBE::new_from_leaves(
                        assoc_n!("type_name" => Atom(type_name)));

                    let mut args = vec![];
                    for individual__arg_res in arg_res {
                        args.push(::util::mbe::EnvMBE::new_from_leaves(
                            assoc_n!("arg" => individual__arg_res.concrete())));
                    }
                    new__tapp_parts.add_anon_repeat(args, None);

                    if let Node(ref f, _, ref exp) = tapp_parts.this_ast {
                        Ok(Ty::new(Node(f.clone(), new__tapp_parts, exp.clone())))
                    } else {
                        panic!("ICE")
                    }
                }
                Some(defined_type) => {
                    // This might ought to be done by a specialized `beta`...
                    expect_ty_node!( (defined_type ; forall_type_0.clone() ; &tapp_parts.this_ast)
                        forall_type__parts;
                        {
                            let params = forall_type__parts.get_rep_leaf_or_panic(&n("param"));
                            if params.len() != arg_res.len() {
                                panic!("Kind error: wrong number of arguments");
                            }
                            let mut new__ty_env = tapp_parts.env.clone();
                            for (name, actual_type) in params.iter().zip(arg_res) {
                                new__ty_env = new__ty_env.set(ast_to_name(name), actual_type);
                            }

                            // This bypasses the binding in the type, which is what we want:
                            synth_type(&::core_forms::strip_ee(
                                    &forall_type__parts.get_leaf_or_panic(&n("body"))),
                                new__ty_env)
                        })
                }
            }
        }),
        Both(LiteralLike, LiteralLike));

    assoc_n!("type" => Rc::new(Biased(Rc::new(forms_to_form_pat![
        fn_type.clone(),
        type_defn("Ident", form_pat!((lit "Ident"))),
        type_defn("Int", form_pat!((lit "Int"))),
        type_defn("Nat", form_pat!((lit "Nat"))),
        type_defn("Float", form_pat!((lit "Float"))),
        enum_type.clone(),
        struct_type.clone(),
        forall_type.clone(),
        dotdotdot_type.clone(),
        mu_type.clone(),
        type_apply.clone(),
        type_by_name.clone()
        ]), Rc::new(VarRef))))
}

#[test]
fn parametric_types() {

    // Are plain parametric types valid?
    without_freshening! { // (so we don't have to compute alpha-equivalence)
    assert_eq!(
        synth_type(&ast!({"type" "forall_type" : "param" => ["t"],
                          "body" => (import [* [forall "param"]] (vr "t"))}),
                   Assoc::new()),
       Ok(ty!({"type" "forall_type" : "param" => ["t"],
               "body" => (import [* [forall "param"]] (vr "t"))})));
   }

    let ident_ty = ty!( { "type" "Ident" : });
    let nat_ty = ty!( { "type" "Nat" : });

    fn tbn(nm: &'static str) -> Ty {
        ty!( { "type" "type_by_name" : "name" => (, ::ast::Ast::Atom(n(nm))) } )
    }

    let para_ty_env = assoc_n!(
        "unary" => ty!({ "type" "forall_type" :
            "param" => ["t"],
            "body" => (import [* [forall "param"]] { "type" "fn" :
                "param" => [ (, nat_ty.concrete()) ],
                "ret" => (, tbn("t").concrete() ) })}),
        "binary" => ty!({ "type" "forall_type" :
            "param" => ["t", "u"],
            "body" => (import [* [forall "param"]] { "type" "fn" :
                "param" => [ (, tbn("t").concrete() ), (, tbn("u").concrete() ) ],
                "ret" => (, nat_ty.concrete()) })}));
    let mued_ty_env = assoc_n!("unary" => ty!((vr "unary")), "binary" => ty!((vr "binary")));

    // If `unary` is `mu`ed, `unary <[ ident ]<` can't be simplified.
    assert_eq!(synth_type(
        &ast!( { "type" "type_apply" :
            "type_name" => "unary",
            "arg" => [ (, ident_ty.concrete()) ]}),
        mued_ty_env.clone()),
        Ok(ty!({ "type" "type_apply" :
            "type_name" => "unary",
            "arg" => [ (, ident_ty.concrete()) ]})));

    // If `unary` is `mu`ed, `unary <[ [nat -> nat] ]<` can't be simplified.
    assert_eq!(synth_type(
        &ast!( { "type" "type_apply" :
            "type_name" => "unary",
            "arg" => [ { "type" "fn" :
                "param" => [(, nat_ty.concrete())], "ret" => (, nat_ty.concrete())} ]}),
        mued_ty_env.clone()),
        Ok(ty!({ "type" "type_apply" :
            "type_name" => "unary",
            "arg" => [ { "type" "fn" :
                "param" => [(, nat_ty.concrete())], "ret" => (, nat_ty.concrete())} ]})));

    // Expand the definition of `unary`.
    assert_eq!(synth_type(
        &ast!( { "type" "type_apply" :
            "type_name" => "unary",
            "arg" => [ (, ident_ty.concrete()) ]}),
        para_ty_env),
        Ok(ty!({ "type" "fn" :
            "param" => [(, nat_ty.concrete() )],
            "ret" => (, ident_ty.concrete())})));
}
