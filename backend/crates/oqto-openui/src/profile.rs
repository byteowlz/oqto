//! The Oqto profile's core set as native clients render it, and the decision between a native
//! render and the part's plain-text fallback.

use oqto_session_view::parts::{PROFILE, UiPart};

use crate::lang::{self, Bounds, Expr, LangError};

/// Profile versions this renderer knows (major.minor); others show the fallback.
pub const KNOWN_VERSIONS: &[&str] = &["0.2"];

/// Core components with their parameters in signature order (`schema/catalog.json` of the
/// profile, 0.2.2). The ADR-0074 core set: text, card, list, table, choice, confirm, text
/// input, form, status.
pub const CORE: &[(&str, &[&str])] = &[
    (
        "Stack",
        &["children", "direction", "gap", "align", "justify", "wrap"],
    ),
    (
        "Card",
        &[
            "children",
            "variant",
            "direction",
            "gap",
            "align",
            "justify",
            "wrap",
        ],
    ),
    ("CardHeader", &["title", "subtitle"]),
    ("TextContent", &["text", "size"]),
    ("MarkDownRenderer", &["textMarkdown", "variant"]),
    ("Label", &["text"]),
    ("Callout", &["variant", "title", "description", "visible"]),
    ("TextCallout", &["variant", "title", "description"]),
    ("Tag", &["text", "icon", "size", "variant"]),
    ("TagBlock", &["tags", "size"]),
    ("Separator", &["orientation", "decorative"]),
    ("ListBlock", &["items", "variant", "size"]),
    (
        "ListItem",
        &["title", "subtitle", "image", "actionLabel", "action"],
    ),
    ("Steps", &["items"]),
    ("StepsItem", &["title", "details"]),
    ("Table", &["columns"]),
    ("Col", &["label", "data", "type"]),
    ("Form", &["name", "buttons", "fields"]),
    ("FormControl", &["label", "input", "hint"]),
    ("Input", &["name", "placeholder", "type", "rules", "value"]),
    (
        "TextArea",
        &["name", "placeholder", "rows", "rules", "value"],
    ),
    (
        "Select",
        &["name", "items", "placeholder", "rules", "value", "size"],
    ),
    ("SelectItem", &["value", "label"]),
    (
        "RadioGroup",
        &["name", "items", "defaultValue", "rules", "value"],
    ),
    ("RadioItem", &["label", "description", "value"]),
    ("CheckBoxGroup", &["name", "items", "rules", "value"]),
    (
        "CheckBoxItem",
        &["label", "description", "name", "defaultChecked"],
    ),
    ("SwitchGroup", &["name", "items", "variant", "value"]),
    (
        "SwitchItem",
        &["label", "description", "name", "defaultChecked"],
    ),
    ("Chips", &["name", "type", "items", "rules", "defaultValue"]),
    ("ChipItem", &["value", "label", "icon", "disabled"]),
    (
        "OptionCards",
        &["name", "type", "items", "rules", "defaultValue"],
    ),
    (
        "OptionCard",
        &["value", "title", "subtitle", "topContent", "disabled"],
    ),
    ("Buttons", &["buttons", "direction"]),
    ("Button", &["label", "action", "variant", "type", "size"]),
];

/// Actions a native render may show (as inert buttons until answers exist).
pub const ACTIONS: &[&str] = &["Answer"];

/// One component with its arguments named.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Component name.
    pub name: String,
    /// Arguments by parameter name, in signature order; absent ones are skipped.
    pub props: Vec<(&'static str, Value)>,
}

/// A resolved argument.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Text.
    Text(String),
    /// A number.
    Number(f64),
    /// A boolean.
    Bool(bool),
    /// `null`.
    Null,
    /// A list.
    List(Vec<Value>),
    /// An object.
    Object(Vec<(String, Value)>),
    /// A component.
    Node(Node),
    /// `@Answer(...)`: shown, never run by the client.
    Action(String),
    /// `$name`.
    State(String),
}

impl Node {
    /// A prop by name.
    pub fn prop(&self, name: &str) -> Option<&Value> {
        self.props.iter().find(|(k, _)| *k == name).map(|(_, v)| v)
    }

    /// A text prop.
    pub fn text(&self, name: &str) -> Option<&str> {
        match self.prop(name) {
            Some(Value::Text(t)) => Some(t),
            _ => None,
        }
    }

    /// Child components of a list prop.
    pub fn children(&self, name: &str) -> Vec<&Self> {
        match self.prop(name) {
            Some(Value::List(items)) => items
                .iter()
                .filter_map(|v| {
                    if let Value::Node(n) = v {
                        Some(n)
                    } else {
                        None
                    }
                })
                .collect(),
            Some(Value::Node(n)) => vec![n],
            _ => Vec::new(),
        }
    }
}

/// Why a part is shown as its fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fallback {
    /// The profile version is not one this renderer knows.
    UnknownProfile(String),
    /// The body could not be parsed or broke a bound.
    Lang(LangError),
    /// A component outside the core set (charts, tabs, galleries, ...).
    Component(String),
    /// An action other than `@Answer`.
    Action(String),
    /// Arguments beyond the signature, or a bare value where a component belongs.
    Shape(String),
}

impl std::fmt::Display for Fallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownProfile(v) => write!(f, "profile {v} is not known here"),
            Self::Lang(e) => write!(f, "{e}"),
            Self::Component(c) => write!(f, "{c} is not in the core set"),
            Self::Action(a) => write!(f, "action @{a} is not supported"),
            Self::Shape(s) => write!(f, "{s}"),
        }
    }
}

fn convert(expr: Expr) -> Result<Value, Fallback> {
    Ok(match expr {
        Expr::Str(s) => Value::Text(s),
        Expr::Num(n) => Value::Number(n),
        Expr::Bool(b) => Value::Bool(b),
        Expr::Null => Value::Null,
        Expr::State(s) => Value::State(s),
        Expr::Array(items) => {
            Value::List(items.into_iter().map(convert).collect::<Result<_, _>>()?)
        }
        Expr::Object(pairs) => Value::Object(
            pairs
                .into_iter()
                .map(|(k, v)| convert(v).map(|v| (k, v)))
                .collect::<Result<_, _>>()?,
        ),
        Expr::Action(name, _) if ACTIONS.contains(&name.as_str()) => Value::Action(name),
        Expr::Action(name, _) => return Err(Fallback::Action(name)),
        Expr::Call(name, args) => Value::Node(node(name, args)?),
        Expr::Ref(name) => return Err(Fallback::Shape(format!("unresolved reference {name}"))),
    })
}

fn node(name: String, args: Vec<Expr>) -> Result<Node, Fallback> {
    let Some((_, params)) = CORE.iter().find(|(n, _)| *n == name) else {
        return Err(Fallback::Component(name));
    };
    if args.len() > params.len() {
        return Err(Fallback::Shape(format!(
            "{name} has {} arguments, the profile allows {}",
            args.len(),
            params.len()
        )));
    }
    let mut props = Vec::new();
    for (param, arg) in params.iter().zip(args) {
        if arg == Expr::Null {
            continue;
        }
        props.push((*param, convert(arg)?));
    }
    Ok(Node { name, props })
}

/// Native render or fallback, for one part.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Every component is in the core set: render the tree.
    Native(Node),
    /// Show the part's fallback text, and why.
    Fallback(Fallback),
}

/// Decide for a body written for `profile_version`.
pub fn decide_body(body: &str, profile_version: &str, bounds: &Bounds) -> Decision {
    let known = KNOWN_VERSIONS.iter().any(|v| {
        profile_version
            .strip_prefix(v)
            .is_some_and(|rest| rest.starts_with('.'))
    });
    if !known {
        return Decision::Fallback(Fallback::UnknownProfile(profile_version.to_owned()));
    }
    let tree = match lang::parse(body, bounds).and_then(|p| p.resolve(bounds)) {
        Ok(tree) => tree,
        Err(error) => return Decision::Fallback(Fallback::Lang(error)),
    };
    match convert(tree) {
        Ok(Value::Node(root)) => Decision::Native(root),
        Ok(_) => Decision::Fallback(Fallback::Shape("root is not a component".to_owned())),
        Err(reason) => Decision::Fallback(reason),
    }
}

/// Decide for a parsed `oqto.ui` entry.
pub fn decide(part: &UiPart) -> Decision {
    if part.part.profile.id != PROFILE {
        return Decision::Fallback(Fallback::UnknownProfile(part.part.profile.id.clone()));
    }
    decide_body(
        &part.part.body,
        &part.part.profile.version,
        &Bounds::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_versions_and_components_fall_back() {
        let b = Bounds::default();
        assert!(matches!(
            decide_body("root = Stack([])", "0.3.0", &b),
            Decision::Fallback(Fallback::UnknownProfile(_))
        ));
        assert!(matches!(
            decide_body("root = Stack([])", "0.2.9", &b),
            Decision::Native(_)
        ));
        assert!(
            matches!(decide_body("root = Stack([BarChart([], [])])", "0.2.2", &b), Decision::Fallback(Fallback::Component(c)) if c == "BarChart")
        );
        assert!(matches!(
            decide_body("root = Button(\"x\", @OpenUrl(\"http://e\"))", "0.2.2", &b),
            Decision::Fallback(Fallback::Action(_))
        ));
        assert!(matches!(
            decide_body("root = \"text\"", "0.2.2", &b),
            Decision::Fallback(Fallback::Shape(_))
        ));
        assert!(matches!(
            decide_body("root = CardHeader(\"a\", \"b\", \"c\")", "0.2.2", &b),
            Decision::Fallback(Fallback::Shape(_))
        ));
    }

    #[test]
    fn arguments_get_their_names() {
        let Decision::Native(root) = decide_body(
            "root = CardHeader(\"Title\", null)",
            "0.2.2",
            &Bounds::default(),
        ) else {
            panic!("native");
        };
        assert_eq!(root.text("title"), Some("Title"));
        assert_eq!(root.prop("subtitle"), None);
    }
}
