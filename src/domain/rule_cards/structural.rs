//! Rule cards for the structural binary checks — part of SRP (BTC, SLM,
//! NMS) and Coupling (OI, SIT, DEH, IET).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "BTC",
        title: "Broken trait contract: all methods are stubs",
        detects: "An impl Trait block where every method body is a stub \
            (todo!/unimplemented!/empty).",
        why: "A contract implemented by stubs satisfies the compiler but \
            lies to callers: the type promises behavior it does not have.",
        fix: "Implement the methods, or remove the impl. If the trait \
            demands methods this type cannot honor, the trait is too wide — \
            split it.",
        suppress: "// qual:allow(srp, btc) reason: \"…\".",
        config: "[structural] check_btc (default true).",
    },
    RuleCard {
        id: "SLM",
        title: "Selfless method: takes self but never references it",
        detects: "A method with a self receiver whose body never uses self \
            — macro bodies are scanned, so self inside format!(…) counts as \
            a reference.",
        why: "A receiver that is never read misstates the method's \
            dependencies and forces callers to have an instance they don't \
            need.",
        fix: "Make it an associated function (drop self) or a free \
            function. If a trait signature forces the receiver, name that \
            in the suppression reason.",
        suppress: "// qual:allow(srp, slm) reason: \"…\".",
        config: "[structural] check_slm (default true).",
    },
    RuleCard {
        id: "NMS",
        title: "Needless &mut self: takes &mut self but never mutates",
        detects: "A method declaring &mut self whose body never mutates \
            self.",
        why: "The needless mut blocks callers holding shared references and \
            overstates the method's effects.",
        fix: "Change the receiver to &self.",
        suppress: "// qual:allow(srp, nms) reason: \"…\".",
        config: "[structural] check_nms (default true).",
    },
    RuleCard {
        id: "OI",
        title: "Orphaned impl: impl block in different file than type",
        detects: "An inherent impl Foo block living in a different file \
            than the struct/enum Foo definition.",
        why: "A type's behavior scattered across files defeats the reader \
            who opens the type to learn what it does.",
        fix: "Move the impl next to its type. For deliberate splits (e.g. \
            one file per concern of a large type), name the reason in a \
            suppression.",
        suppress: "// qual:allow(coupling, oi) reason: \"…\".",
        config: "[structural] check_oi (default true).",
    },
    RuleCard {
        id: "SIT",
        title: "Single-impl trait: non-pub trait with exactly one implementation",
        detects: "A non-pub trait implemented exactly once (#[cfg(test)] \
            impls count, so test seams with a test double are not flagged).",
        why: "An abstraction with one realization adds indirection without \
            offering choice — readers must follow the trait to find the one \
            place the behavior lives.",
        fix: "Inline the trait into its single implementor. Keep it only as \
            a deliberate seam (future second impl, test boundary) and say \
            so.",
        suppress: "// qual:allow(coupling, sit) reason: \"…\".",
        config: "[structural] check_sit (default true).",
    },
    RuleCard {
        id: "DEH",
        title: "Downcast escape hatch: use of Any::downcast",
        detects: "A call to Any::downcast/downcast_ref/downcast_mut.",
        why: "Downcasting reintroduces the type knowledge the abstraction \
            was supposed to hide; each cast is a place the design leaks.",
        fix: "Model the alternatives explicitly — an enum over the known \
            variants, or a trait method that moves the behavior to where \
            the concrete type is known.",
        suppress: "// qual:allow(coupling, deh) reason: \"…\" (e.g. a plugin \
            boundary that genuinely requires dynamic typing).",
        config: "[structural] check_deh (default true).",
    },
    RuleCard {
        id: "IET",
        title: "Inconsistent error types in module",
        detects: "A module whose public functions return several unrelated \
            error types.",
        why: "Callers must learn one error contract per function instead of \
            one per module; conversions multiply at every call site.",
        fix: "Define one module error enum (thiserror) with From \
            conversions for the underlying sources; return it from every \
            pub fn.",
        suppress: "// qual:allow(coupling, iet) reason: \"…\".",
        config: "[structural] check_iet (default true).",
    },
];
