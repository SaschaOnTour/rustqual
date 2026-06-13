//! Rule cards for the boilerplate patterns (BP-*), part of the DRY dimension.

use super::RuleCard;

/// Shared suppress line: every BP pattern is one finding-kind (`boilerplate`).
const BP_SUPPRESS: &str = "// qual:allow(dry, boilerplate) reason: \"…\".";

/// Shared config line: patterns are individually selectable.
const BP_CONFIG: &str = "[boilerplate] patterns — list only the BP ids you \
    want active (empty list = all); suggest_crates toggles crate \
    recommendations in suggestions.";

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "BP-001",
        title: "Trivial From implementation (derivable)",
        detects: "An impl From<A> for B whose from() only maps fields \
            one-to-one, with no logic.",
        why: "Hand-written field mapping drifts when either struct changes; \
            a declarative form keeps the conversion total by construction.",
        fix: "derive_more::From, or struct-update / field-shorthand \
            construction. Keep a manual impl only when it documents a real \
            conversion contract.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-002",
        title: "Trivial Display implementation (derivable)",
        detects: "An impl Display whose single fmt method is one \
            write!/writeln! call with no further logic.",
        why: "The same effect is available declaratively; N hand-rolled \
            trivial Displays drift in style and bury the one impl that \
            actually formats.",
        fix: "derive_more::Display with #[display(\"…\")], or a project-wide \
            dependency-free idiom (e.g. f.write_str(&self.0) for \
            string-backed newtypes, or one local macro_rules! emitting the \
            impl for all newtypes).",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-003",
        title: "Trivial getter/setter (consider field visibility)",
        detects: "Methods that only return or assign a single field, with no \
            validation or transformation.",
        why: "Accessor ceremony without invariants adds indirection but no \
            encapsulation value.",
        fix: "Make the field pub(crate)/pub where appropriate, or use a \
            getter/setter derive; keep manual accessors only where they \
            guard an invariant.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-004",
        title: "Builder pattern (consider derive macro)",
        detects: "A struct with many builder-style methods (set one field, \
            return self).",
        why: "Hand-rolled builders are dozens of lines of mechanical code \
            that must be extended for every new field.",
        fix: "derive_builder / typed-builder, or a constructor taking the \
            required fields plus Default for the rest.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-005",
        title: "Manual Default implementation (derivable)",
        detects: "An impl Default whose default() only fills fields with \
            literals or nested Default::default() calls.",
        why: "The derive expresses the same thing shorter and stays correct \
            when fields are added.",
        fix: "#[derive(Default)]; for non-zero defaults use field-level \
            #[default] (enums) or keep only the divergent fields manual via \
            a const.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-006",
        title: "Repetitive match mapping",
        detects: "A match that maps enum variants one-to-one onto values or \
            another enum's variants, arm after arm with the same shape.",
        why: "Mechanical mappings hide in plain sight among real logic and \
            must be extended for every new variant.",
        fix: "A lookup table (const array / match in one dedicated fn), a \
            strum derive, or storing the mapped value in the variant itself.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-007",
        title: "Error enum boilerplate (consider thiserror)",
        detects: "An error enum accompanied by several trivial From impls \
            (and manual Display/Error plumbing).",
        why: "thiserror generates exactly this plumbing from attributes, \
            keeping the enum readable as a list of failure cases.",
        fix: "#[derive(thiserror::Error)] with #[error(\"…\")] and #[from] \
            on the source fields.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-008",
        title: "Clone-heavy conversion",
        detects: "A struct construction cloning many fields from another \
            value.",
        why: "Field-by-field cloning usually signals a conversion that \
            should own or borrow its input — each clone is a hidden \
            allocation and a sign the ownership design fights the data flow.",
        fix: "Take the source by value and move the fields, implement \
            From/Into, or borrow with lifetimes where the copy is not \
            needed.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-009",
        title: "Struct update boilerplate",
        detects: "A function constructing the same struct type several times \
            with mostly overlapping fields, instead of deriving the variants \
            from a base value.",
        why: "Repeating the unchanged fields obscures which field actually \
            differs between the constructions — the one thing the reader \
            needs to see.",
        fix: "Build one base value and use struct-update syntax for the \
            variants: Type { changed, ..base } (Clone the base if needed).",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
    RuleCard {
        id: "BP-010",
        title: "Format string repetition",
        detects: "The same format string literal used in many \
            format!/println!-family calls across a file.",
        why: "A repeated format string is a de-facto template; copies drift \
            in wording and there is no single place to change the format.",
        fix: "Extract the string into a const, or wrap the formatting in a \
            small helper function that owns the template.",
        suppress: BP_SUPPRESS,
        config: BP_CONFIG,
    },
];
