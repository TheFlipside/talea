//! Spending/income categories.
//!
//! Categories are **global** (shared across all accounts) and purely
//! descriptive — used for classification and the stats screen, never as
//! envelopes or limits (see `docs/DESIGN.md` §1).

use serde::{Deserialize, Serialize};

use crate::domain::error::{DomainError, MAX_LABEL_LEN};
use crate::domain::ids::CategoryId;

/// A category's visual marker: either a preset icon id or a literal emoji.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryIcon {
    /// Identifier into a curated preset icon set (resolved by the frontend).
    Preset(String),
    /// A literal emoji, e.g. `"🛒"`.
    Emoji(String),
}

impl CategoryIcon {
    fn validate(&self) -> Result<(), DomainError> {
        let value = match self {
            Self::Preset(v) | Self::Emoji(v) => v,
        };
        let len = value.chars().count();
        if len == 0 {
            return Err(DomainError::EmptyLabel);
        }
        if len > MAX_LABEL_LEN {
            return Err(DomainError::LabelTooLong {
                len,
                max: MAX_LABEL_LEN,
            });
        }
        Ok(())
    }
}

/// A category in the global list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "CategoryRepr")]
pub struct Category {
    id: CategoryId,
    label: String,
    icon: CategoryIcon,
}

#[derive(Deserialize)]
struct CategoryRepr {
    id: CategoryId,
    label: String,
    icon: CategoryIcon,
}

impl TryFrom<CategoryRepr> for Category {
    type Error = DomainError;

    fn try_from(repr: CategoryRepr) -> Result<Self, Self::Error> {
        Self::new(repr.id, repr.label, repr.icon)
    }
}

impl Category {
    /// Creates a category.
    ///
    /// # Errors
    ///
    /// - [`DomainError::EmptyLabel`] if `label` (or the icon's value) is empty.
    /// - [`DomainError::LabelTooLong`] if `label` (or the icon's value) exceeds
    ///   [`MAX_LABEL_LEN`] characters.
    pub fn new(id: CategoryId, label: String, icon: CategoryIcon) -> Result<Self, DomainError> {
        let len = label.chars().count();
        if len == 0 {
            return Err(DomainError::EmptyLabel);
        }
        if len > MAX_LABEL_LEN {
            return Err(DomainError::LabelTooLong {
                len,
                max: MAX_LABEL_LEN,
            });
        }
        icon.validate()?;
        Ok(Self { id, label, icon })
    }

    /// Stable identifier.
    #[must_use]
    pub const fn id(&self) -> CategoryId {
        self.id
    }

    /// Display label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Visual marker.
    #[must_use]
    pub const fn icon(&self) -> &CategoryIcon {
        &self.icon
    }
}

#[cfg(test)]
mod tests {
    use super::{Category, CategoryIcon};
    use crate::domain::error::DomainError;
    use crate::domain::ids::CategoryId;

    #[test]
    fn constructs_and_round_trips() {
        let cat = Category::new(
            CategoryId::new(3),
            "Groceries".to_owned(),
            CategoryIcon::Emoji("🛒".to_owned()),
        )
        .unwrap();
        let json = serde_json::to_string(&cat).unwrap();
        assert!(json.contains(r#""icon":{"emoji":"🛒"}"#));
        assert_eq!(serde_json::from_str::<Category>(&json).unwrap(), cat);
    }

    #[test]
    fn preset_icon_variant() {
        let cat = Category::new(
            CategoryId::new(1),
            "Rent".to_owned(),
            CategoryIcon::Preset("home".to_owned()),
        )
        .unwrap();
        let json = serde_json::to_string(&cat).unwrap();
        assert!(json.contains(r#""icon":{"preset":"home"}"#));
    }

    #[test]
    fn rejects_empty_and_overlong_label() {
        assert_eq!(
            Category::new(
                CategoryId::new(1),
                String::new(),
                CategoryIcon::Preset("x".to_owned())
            ),
            Err(DomainError::EmptyLabel)
        );
        assert!(matches!(
            Category::new(
                CategoryId::new(1),
                "x".repeat(201),
                CategoryIcon::Preset("x".to_owned())
            ),
            Err(DomainError::LabelTooLong { .. })
        ));
    }

    #[test]
    fn rejects_empty_icon_value() {
        assert_eq!(
            Category::new(
                CategoryId::new(1),
                "Valid".to_owned(),
                CategoryIcon::Emoji(String::new())
            ),
            Err(DomainError::EmptyLabel)
        );
    }
}
