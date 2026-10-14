//! The document knowledge the hub validates against (spec §3.6 phase A: `DocSchema`): which
//! kinds exist, which parents accept children — slides under the root, shapes under slides,
//! shapes under groups.

use craft_collab::adapter::ValidationCtx;
use craft_collab::change::Change;
use craft_collab::limits::Limits;

/// The deckcraft document schema.
#[derive(Debug, Default)]
pub struct DeckSchema {
    limits: Limits,
}

impl craft_collab::adapter::DocSchema for DeckSchema {
    fn validate(&self, change: &Change, ctx: &ValidationCtx<'_>) -> Result<(), String> {
        match change {
            Change::Create { kind, parent, props, .. } => {
                if !crate::KINDS.contains(&kind.as_ref()) {
                    return Err(format!("unknown kind '{kind}'"));
                }
                if *parent == crate::ROOT {
                    if kind.as_ref() != "slide" {
                        return Err(format!("'{kind}' does not attach to the file root; only slides do"));
                    }
                } else {
                    let p = ctx.replica.node(*parent).ok_or_else(|| format!("unknown parent {parent:?}"))?;
                    let accepts = if p.kind.as_ref() == "slide" {
                        crate::KINDS.contains(&kind.as_ref()) && kind.as_ref() != "slide"
                    } else {
                        p.kind.as_ref() == "group"
                    };
                    if !accepts {
                        return Err(format!("a '{}' accepts no '{}'", p.kind, kind));
                    }
                }
                for (p, _) in props {
                    let in_table = (kind.as_ref() == "slide" && crate::props::SLIDE_PROPS.contains(&p.as_ref()))
                        || (kind.as_ref() != "slide" && crate::props::SHAPE_PROPS.contains(&p.as_ref()));
                    if p.as_ref() != "parent" && p.as_ref() != "position" && !in_table {
                        return Err(format!("unknown property '{p}' for '{kind}'"));
                    }
                }
                Ok(())
            }
            Change::Set { prop, .. } => {
                if prop.as_ref() == "parent" || prop.as_ref() == "position" {
                    return Ok(());
                }
                if crate::props::SHAPE_PROPS.contains(&prop.as_ref()) || crate::props::SLIDE_PROPS.contains(&prop.as_ref()) {
                    Ok(())
                } else {
                    Err(format!("unknown property '{prop}'"))
                }
            }
            Change::Delete { .. } => Ok(()),
            Change::SetDoc { prop, .. } => {
                if crate::props::DOC_PROPS.contains(&prop.as_ref()) {
                    Ok(())
                } else {
                    Err(format!("unknown document property '{prop}'"))
                }
            }
            Change::Put { .. } | Change::Remove { .. } => Err("deckcraft has no document-level entry lists in the first cut".into()),
        }
    }

    fn recompute(&self, _touched: &[craft_collab::ids::NodeId]) -> Vec<Change> {
        // DeckCraft derives nothing automatically today: autolayout fits, diagram regeneration
        // and placeholder inheritance are explicit commands the author runs, not properties the
        // presentation recomputes on its own — so there is no author-only recompute to compose
        // (spec §5.7's map decides: "commands, not derived state"). When a derived property
        // lands, its recompute belongs here and nowhere else.
        Vec::new()
    }

    fn limits(&self) -> &Limits {
        &self.limits
    }
}
