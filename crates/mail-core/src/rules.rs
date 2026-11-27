//! Client-side mail rules & auto-labeling evaluator.

use serde::{Deserialize, Serialize};

use crate::ids::LabelId;
use crate::types::InboxCategory;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuleCondition {
    FromContains { value: String },
    DomainEquals { value: String },
    SubjectContains { value: String },
    BodyContains { value: String },
    HasAttachment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuleAction {
    ApplyLabel { label_id: LabelId },
    Archive,
    Trash,
    MarkRead,
    Star,
    SetCategory { category: InboxCategory },
}

pub(crate) struct RuleEvalTarget<'a> {
    pub from_email: &'a str,
    pub from_name: Option<&'a str>,
    pub subject: &'a str,
    pub body_text: &'a str,
    pub has_attachments: bool,
}

pub(crate) fn matches_rule(
    conditions: &[RuleCondition],
    match_all: bool,
    target: &RuleEvalTarget<'_>,
) -> bool {
    if conditions.is_empty() {
        return false;
    }
    let check = |cond: &RuleCondition| -> bool {
        match cond {
            RuleCondition::FromContains { value } => {
                let needle = value.trim().to_ascii_lowercase();
                if needle.is_empty() {
                    return false;
                }
                target.from_email.to_ascii_lowercase().contains(&needle)
                    || target
                        .from_name
                        .map(|n| n.to_ascii_lowercase().contains(&needle))
                        .unwrap_or(false)
            }
            RuleCondition::DomainEquals { value } => {
                let needle = value.trim().trim_start_matches('@').to_ascii_lowercase();
                if needle.is_empty() {
                    return false;
                }
                target
                    .from_email
                    .rsplit_once('@')
                    .map(|(_, dom)| dom.trim().eq_ignore_ascii_case(&needle))
                    .unwrap_or(false)
            }
            RuleCondition::SubjectContains { value } => {
                let needle = value.trim().to_ascii_lowercase();
                !needle.is_empty() && target.subject.to_ascii_lowercase().contains(&needle)
            }
            RuleCondition::BodyContains { value } => {
                let needle = value.trim().to_ascii_lowercase();
                !needle.is_empty() && target.body_text.to_ascii_lowercase().contains(&needle)
            }
            RuleCondition::HasAttachment => target.has_attachments,
        }
    };

    if match_all {
        conditions.iter().all(check)
    } else {
        conditions.iter().any(check)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_and_and_or_rule_conditions() {
        let target = RuleEvalTarget {
            from_email: "billing@figma.com",
            from_name: Some("Figma Billing"),
            subject: "Your Figma invoice #900",
            body_text: "Attached is your PDF receipt.",
            has_attachments: true,
        };

        assert!(matches_rule(
            &[
                RuleCondition::DomainEquals {
                    value: "figma.com".into()
                },
                RuleCondition::SubjectContains {
                    value: "invoice".into()
                },
                RuleCondition::HasAttachment,
            ],
            true,
            &target
        ));

        assert!(!matches_rule(
            &[
                RuleCondition::DomainEquals {
                    value: "stripe.com".into()
                },
                RuleCondition::HasAttachment,
            ],
            true,
            &target
        ));

        assert!(matches_rule(
            &[
                RuleCondition::DomainEquals {
                    value: "stripe.com".into()
                },
                RuleCondition::HasAttachment,
            ],
            false,
            &target
        ));
    }
}
