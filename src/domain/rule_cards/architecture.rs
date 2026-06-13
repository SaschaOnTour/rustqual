//! Rule cards for the Architecture dimension (hierarchical
//! `architecture/<family>[/<sub-kind>]` ids; only base ids carry cards —
//! pattern/trait_contract sub-kinds are user-defined rule names).

use super::RuleCard;

pub(super) const CARDS: &[RuleCard] = &[
    RuleCard {
        id: "architecture/layer",
        title: "Layer rule violation",
        detects: "A file importing against its layer's allowed direction, \
            as defined by the [architecture.layers] ranks (inner layers \
            must not depend on outer ones).",
        why: "Layering only protects the core if every edge respects it; \
            one inward-pointing import couples the domain to its \
            adapters.",
        fix: "Move the needed code inward, invert the dependency with a \
            port (trait) defined in the inner layer, or relocate the file \
            to the layer it actually belongs to.",
        suppress: "// qual:allow(architecture, layer) reason: \"…\".",
        config: "[architecture.layers] defines globs and ranks; \
            [architecture.reexport_points] exempts composition roots.",
    },
    RuleCard {
        id: "architecture/layer/unmatched",
        title: "File doesn't match any configured layer",
        detects: "A production file whose path matches no \
            [architecture.layers] glob, under unmatched_behavior = \
            \"strict_error\".",
        why: "An unmatched file is outside the architecture: its \
            dependencies are checked against nothing, so the layer rule \
            silently does not apply to it.",
        fix: "Move the file into a layer directory, extend the layer's \
            glob, or register it under [architecture.reexport_points] if \
            it is composition-root code.",
        suppress: "// qual:allow(architecture, layer) reason: \"…\" (the \
            layer family covers unmatched findings).",
        config: "[architecture] unmatched_behavior (strict_error | warn | \
            ignore); [architecture.layers] globs.",
    },
    RuleCard {
        id: "architecture/forbidden",
        title: "Forbidden-edge violation between layers",
        detects: "An import matching a [[architecture.forbidden]] rule — \
            an explicitly banned from-glob → to-glob edge.",
        why: "Forbidden edges encode project-specific decisions the \
            generic layer ranks cannot express; a hit is a documented \
            decision being violated.",
        fix: "Follow the rule's own message (each forbidden rule carries \
            its rationale); typically: route through the sanctioned \
            module instead of importing the banned one directly.",
        suppress: "// qual:allow(architecture, forbidden) reason: \"…\".",
        config: "[[architecture.forbidden]] rules (from/to globs + \
            message).",
    },
    RuleCard {
        id: "architecture/pattern",
        title: "Symbol-pattern violation (path/method/macro)",
        detects: "A use of a forbidden symbol matching a \
            [[architecture.pattern]] rule: forbid_path_prefix, \
            forbid_method_call, forbid_macro_call, or forbid_glob_import \
            (prelude globs exempt unless allow_prelude_glob = false). \
            Macro bodies are scanned via structured recovery.",
        why: "Pattern rules ban concrete escape hatches (panic in \
            production, println debugging, direct DB access) that layer \
            globs cannot see.",
        fix: "Each pattern rule carries its own message naming the \
            sanctioned alternative — use that. The finding's sub-id names \
            the violated rule.",
        suppress: "// qual:allow(architecture, pattern) reason: \"…\".",
        config: "[[architecture.pattern]] rules (name, kind, value, \
            message, allow_prelude_glob).",
    },
    RuleCard {
        id: "architecture/trait_contract",
        title: "Trait-contract violation",
        detects: "A trait failing a [[architecture.trait_contract]] check \
            (e.g. object_safety, required method set) configured for it.",
        why: "Contract checks pin design properties of key traits — losing \
            object safety or a required method breaks downstream \
            consumers at a distance.",
        fix: "Restore the property the check names (the sub-id carries \
            the check kind), or update the contract rule if the design \
            legitimately moved on.",
        suppress: "// qual:allow(architecture, trait_contract) reason: \
            \"…\".",
        config: "[[architecture.trait_contract]] rules (trait name + \
            check).",
    },
    RuleCard {
        id: "architecture/call_parity/no_delegation",
        title: "Adapter pub fn does not reach the target layer",
        detects: "Check A: an adapter's pub fn whose call graph never \
            reaches the configured target layer.",
        why: "An adapter entry point that does its own work instead of \
            delegating duplicates business logic outside the layer that \
            owns it.",
        fix: "Route the entry point through the target layer's API; move \
            any local logic inward.",
        suppress: "// qual:allow(architecture, call_parity) reason: \"…\".",
        config: "[architecture.call_parity] (adapter globs, target layer; \
            see book/adapter-parity.md).",
    },
    RuleCard {
        id: "architecture/call_parity/missing_adapter",
        title: "Target pub fn is not reached by every adapter (or is orphaned)",
        detects: "Check B: a target-layer pub fn present in one adapter's \
            coverage but missing from another, or transitively unreachable \
            from any adapter touchpoint (dead island).",
        why: "Parity gaps mean one interface (CLI, MCP, HTTP, …) silently \
            lacks a capability the others expose.",
        fix: "Wire the missing adapter route to the target fn, or remove \
            the orphaned fn if no adapter should expose it.",
        suppress: "// qual:allow(architecture, call_parity) reason: \"…\".",
        config: "[architecture.call_parity] (see book/adapter-parity.md).",
    },
    RuleCard {
        id: "architecture/call_parity/multi_touchpoint",
        title: "Adapter pub fn has multiple target touchpoints",
        detects: "Check C: an adapter entry point calling into the target \
            layer at more than one place (severity configurable via \
            single_touchpoint, default warn).",
        why: "Several touchpoints mean the adapter is composing business \
            steps itself — orchestration that belongs in the target \
            layer.",
        fix: "Introduce one target-layer function that composes the steps; \
            call only it from the adapter.",
        suppress: "// qual:allow(architecture, call_parity) reason: \"…\".",
        config: "[architecture.call_parity] single_touchpoint (error | \
            warn | off).",
    },
    RuleCard {
        id: "architecture/call_parity/multiplicity_mismatch",
        title: "Target pub fn reached with divergent handler counts across adapters",
        detects: "Check D: every adapter reaches the target fn, but with \
            different handler counts (e.g. cli=2, mcp=1).",
        why: "Divergent multiplicity usually means one adapter splits a \
            capability the others treat as one — the interfaces have \
            quietly stopped being parallel.",
        fix: "Align the adapters on one route per capability, or document \
            the deliberate asymmetry in a suppression reason.",
        suppress: "// qual:allow(architecture, call_parity) reason: \"…\".",
        config: "[architecture.call_parity] (see book/adapter-parity.md).",
    },
];
