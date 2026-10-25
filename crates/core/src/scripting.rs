//! Platform-neutral scripting vocabulary and deterministic object resolution.
//! Four-character terms mirror the reference dictionary; no OS handles live here.
use std::collections::BTreeMap;
pub const fn code(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}
pub type Record = BTreeMap<u32, Value>;
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Text(String),
    Int(i64),
    Bool(bool),
    Code(u32),
    List(Vec<Value>),
    Record(u32, Record),
    Object(Object),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Object {
    Application,
    Window(String),
    Tab { window: String, id: String },
    Space { window: String, id: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    Index(i64),
    Id(String),
    Name(String),
    All,
    Last,
    Middle,
    Any,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Application,
    Elements {
        class: u32,
        container: Box<Query>,
        selector: Selector,
    },
    Property {
        property: u32,
        container: Box<Query>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Get(Query),
    Count(Query, Option<u32>),
    Set(Query, Value),
    Make {
        class: u32,
        container: Query,
        properties: Record,
    },
    Close(Query),
    Select(Query),
    Back(Query),
    Forward(Query),
    Reload(Query),
    Stop(Query),
    Execute(Query, String),
    Focus(Query),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub number: i32,
    pub message: String,
}
impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            number: -1700,
            message: message.into(),
        }
    }
    pub fn missing() -> Self {
        Self {
            number: -1728,
            message: "The requested object does not exist.".into(),
        }
    }
    pub fn unsupported() -> Self {
        Self {
            number: -1708,
            message: "This scripting operation is not supported.".into(),
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;
impl Value {
    pub fn text(&self) -> Result<&str> {
        if let Self::Text(s) = self {
            Ok(s)
        } else {
            Err(Error::invalid("Expected text."))
        }
    }
    pub fn type_code(&self) -> Result<u32> {
        if let Self::Code(c) = self {
            Ok(*c)
        } else {
            Err(Error::invalid("Expected a class or property code."))
        }
    }
}
pub fn query(value: &Value) -> Result<Query> {
    query_depth(value, 0)
}
fn query_depth(value: &Value, depth: usize) -> Result<Query> {
    if depth > 32 {
        return Err(Error::invalid("Object specifier nesting is too deep."));
    }
    if *value == Value::Null {
        return Ok(Query::Application);
    }
    if let Value::Object(object) = value {
        return Ok(object.query());
    }
    let Value::Record(kind, r) = value else {
        return Err(Error::invalid("Expected an object specifier."));
    };
    if *kind != code(b"obj ") {
        return Err(Error::invalid("Expected an object specifier."));
    }
    let class = r
        .get(&code(b"want"))
        .ok_or_else(Error::missing)?
        .type_code()?;
    let container = Box::new(query_depth(
        r.get(&code(b"from")).unwrap_or(&Value::Null),
        depth + 1,
    )?);
    let form = r
        .get(&code(b"form"))
        .ok_or_else(Error::missing)?
        .type_code()?;
    let selection = r.get(&code(b"seld")).ok_or_else(Error::missing)?;
    if form == code(b"prop") {
        return Ok(Query::Property {
            property: selection.type_code()?,
            container,
        });
    }
    let selector = if form == code(b"ID  ") {
        Selector::Id(selection.text()?.into())
    } else if form == code(b"name") {
        Selector::Name(selection.text()?.into())
    } else if form == code(b"indx") {
        match selection {
            Value::Int(n) => Selector::Index(*n),
            Value::Code(c) if *c == code(b"all ") => Selector::All,
            Value::Code(c) if *c == code(b"firs") => Selector::Index(1),
            Value::Code(c) if *c == code(b"last") => Selector::Last,
            Value::Code(c) if *c == code(b"midd") => Selector::Middle,
            Value::Code(c) if *c == code(b"any ") => Selector::Any,
            _ => return Err(Error::invalid("Unsupported object index.")),
        }
    } else {
        return Err(Error::invalid("Unsupported object selection."));
    };
    Ok(Query::Elements {
        class,
        container,
        selector,
    })
}
impl Object {
    pub fn query(&self) -> Query {
        match self {
            Self::Application => Query::Application,
            Self::Window(id) => Query::Elements {
                class: code(b"WiND"),
                container: Box::new(Query::Application),
                selector: Selector::Id(id.clone()),
            },
            Self::Tab { window, id } | Self::Space { window, id } => Query::Elements {
                class: if matches!(self, Self::Tab { .. }) {
                    code(b"tAbB")
                } else {
                    code(b"sPaC")
                },
                container: Box::new(Self::Window(window.clone()).query()),
                selector: Selector::Id(id.clone()),
            },
        }
    }
}
impl Command {
    pub fn decode(class: u32, id: u32, params: &Record) -> Result<Self> {
        let direct = params.get(&code(b"----")).unwrap_or(&Value::Null);
        let q = || query(direct);
        if class == code(b"core") {
            return Ok(if id == code(b"getd") {
                Self::Get(q()?)
            } else if id == code(b"cnte") {
                Self::Count(
                    q()?,
                    params
                        .get(&code(b"kocl"))
                        .map(Value::type_code)
                        .transpose()?,
                )
            } else if id == code(b"setd") {
                Self::Set(
                    q()?,
                    params
                        .get(&code(b"data"))
                        .ok_or_else(Error::missing)?
                        .clone(),
                )
            } else if id == code(b"clos") {
                Self::Close(q()?)
            } else if id == code(b"crel") {
                let class = params
                    .get(&code(b"kocl"))
                    .ok_or_else(Error::missing)?
                    .type_code()?;
                let properties = match params.get(&code(b"prdt")) {
                    Some(Value::Record(_, r)) => r.clone(),
                    None => Record::new(),
                    _ => return Err(Error::invalid("Expected a properties record.")),
                };
                let container = if let Some(Value::Record(_, r)) = params.get(&code(b"insh")) {
                    query(r.get(&code(b"kobj")).unwrap_or(&Value::Null))?
                } else {
                    query(params.get(&code(b"subj")).unwrap_or(&Value::Null))?
                };
                Self::Make {
                    class,
                    container,
                    properties,
                }
            } else {
                return Err(Error::unsupported());
            });
        }
        Ok(if class == code(b"CrSe") && id == code(b"lect") {
            Self::Select(q()?)
        } else if class == code(b"SpaF") && id == code(b"ocus") {
            Self::Focus(q()?)
        } else if class == code(b"CrSu") {
            if id == code(b"Back") {
                Self::Back(q()?)
            } else if id == code(b"Fwd ") {
                Self::Forward(q()?)
            } else if id == code(b"Rlod") {
                Self::Reload(q()?)
            } else if id == code(b"stop") {
                Self::Stop(q()?)
            } else if id == code(b"ExJa") {
                Self::Execute(
                    q()?,
                    params
                        .get(&code(b"JvSc"))
                        .ok_or_else(Error::missing)?
                        .text()?
                        .into(),
                )
            } else {
                return Err(Error::unsupported());
            }
        } else {
            return Err(Error::unsupported());
        })
    }
}
#[derive(Clone, Debug)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub location: String,
}
#[derive(Clone, Debug)]
pub struct Space {
    pub id: String,
    pub title: String,
    pub tabs: Vec<Tab>,
}
#[derive(Clone, Debug)]
pub struct Window {
    pub id: String,
    pub title: String,
    pub mode: String,
    pub active_tab: Option<String>,
    pub active_space: Option<String>,
    pub tabs: Vec<Tab>,
    pub spaces: Vec<Space>,
    pub minimized: bool,
    pub visible: bool,
    pub zoomed: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub name: String,
    pub version: String,
    pub frontmost: bool,
    pub windows: Vec<Window>,
}
impl Snapshot {
    pub fn resolve(&self, q: &Query) -> Result<(Vec<Object>, bool)> {
        match q {
            Query::Application => Ok((vec![Object::Application], false)),
            Query::Property {
                property,
                container,
            } => {
                let (parents, plural) = self.resolve(container)?;
                let mut objects = Vec::new();
                for p in parents {
                    match self.property(&p, *property)? {
                        Value::Object(o) => objects.push(o),
                        Value::Null => {}
                        _ => return Err(Error::invalid("Property is not an object.")),
                    }
                }
                Ok((objects, plural))
            }
            Query::Elements {
                class,
                container,
                selector,
            } => {
                let (parents, parent_plural) = self.resolve(container)?;
                let mut result = Vec::new();
                for parent in parents {
                    let mut elements = self.elements(&parent, *class)?;
                    if *class == code(b"tAbB") && matches!(selector, Selector::Id(_)) {
                        if let Object::Window(window) = &parent {
                            for space in &self.window(window)?.spaces {
                                for tab in &space.tabs {
                                    let object = Object::Tab {
                                        window: window.clone(),
                                        id: tab.id.clone(),
                                    };
                                    if !elements.contains(&object) {
                                        elements.push(object)
                                    }
                                }
                            }
                        }
                    }
                    if *selector == Selector::All {
                        result.extend(elements);
                        continue;
                    }
                    let index = match selector {
                        Selector::Index(n) if *n > 0 => usize::try_from(*n - 1).ok(),
                        Selector::Index(n) if *n < 0 => {
                            elements.len().checked_sub(n.unsigned_abs() as usize)
                        }
                        Selector::Last => elements.len().checked_sub(1),
                        Selector::Middle => elements.len().checked_sub(1).map(|n| n / 2),
                        Selector::Any => Some(0),
                        Selector::Id(id) => elements.iter().position(|o| {
                            self.property(o, code(b"ID  ")).ok() == Some(Value::Text(id.clone()))
                        }),
                        Selector::Name(name) => elements.iter().position(|o| {
                            self.property(o, code(b"pnam")).ok() == Some(Value::Text(name.clone()))
                        }),
                        _ => None,
                    };
                    let Some(index) = index.filter(|n| *n < elements.len()) else {
                        return Err(Error::missing());
                    };
                    result.push(elements.remove(index));
                }
                Ok((result, parent_plural || *selector == Selector::All))
            }
        }
    }
    pub fn one(&self, q: &Query) -> Result<Object> {
        let (mut r, _) = self.resolve(q)?;
        if r.len() != 1 {
            return Err(Error::missing());
        }
        Ok(r.remove(0))
    }
    pub fn get(&self, q: &Query) -> Result<Value> {
        if let Query::Property {
            property,
            container,
        } = q
        {
            let (objects, plural) = self.resolve(container)?;
            let values = objects
                .iter()
                .map(|o| self.property(o, *property))
                .collect::<Result<Vec<_>>>()?;
            return Ok(if plural {
                Value::List(values)
            } else {
                values.into_iter().next().unwrap_or(Value::Null)
            });
        }
        let (objects, plural) = self.resolve(q)?;
        let values = objects.into_iter().map(Value::Object).collect::<Vec<_>>();
        Ok(if plural {
            Value::List(values)
        } else {
            values.into_iter().next().unwrap_or(Value::Null)
        })
    }
    pub fn count(&self, q: &Query, class: Option<u32>) -> Result<Value> {
        let (objects, _) = self.resolve(q)?;
        let count = if let Some(class) = class {
            objects
                .iter()
                .map(|o| self.elements(o, class).map(|v| v.len()))
                .sum::<Result<usize>>()?
        } else {
            objects.len()
        };
        Ok(Value::Int(count as i64))
    }
    fn window(&self, id: &str) -> Result<&Window> {
        self.windows
            .iter()
            .find(|w| w.id == id)
            .ok_or_else(Error::missing)
    }
    fn elements(&self, parent: &Object, class: u32) -> Result<Vec<Object>> {
        match parent {
            Object::Application if class == code(b"WiND") => Ok(self
                .windows
                .iter()
                .map(|w| Object::Window(w.id.clone()))
                .collect()),
            Object::Window(id) => {
                let w = self.window(id)?;
                if class == code(b"tAbB") {
                    Ok(w.tabs
                        .iter()
                        .map(|t| Object::Tab {
                            window: id.clone(),
                            id: t.id.clone(),
                        })
                        .collect())
                } else if class == code(b"sPaC") {
                    Ok(w.spaces
                        .iter()
                        .map(|s| Object::Space {
                            window: id.clone(),
                            id: s.id.clone(),
                        })
                        .collect())
                } else {
                    Err(Error::unsupported())
                }
            }
            Object::Space { window, id } if class == code(b"tAbB") => {
                let s = self
                    .window(window)?
                    .spaces
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or_else(Error::missing)?;
                Ok(s.tabs
                    .iter()
                    .map(|t| Object::Tab {
                        window: window.clone(),
                        id: t.id.clone(),
                    })
                    .collect())
            }
            _ => Err(Error::unsupported()),
        }
    }
    pub fn property(&self, o: &Object, p: u32) -> Result<Value> {
        if p == code(b"pcls") {
            return Ok(Value::Code(match o {
                Object::Application => code(b"capp"),
                Object::Window(_) => code(b"WiND"),
                Object::Tab { .. } => code(b"tAbB"),
                Object::Space { .. } => code(b"sPaC"),
            }));
        }
        if p == code(b"pALL") {
            let keys: &[[u8; 4]] = match o {
                Object::Application => &[*b"pnam", *b"vers", *b"pisf"],
                Object::Window(_) => &[
                    *b"ID  ", *b"pnam", *b"pidx", *b"hclb", *b"ismn", *b"pmnd", *b"prsz", *b"pvis",
                    *b"iszm", *b"pzum", *b"acTa", *b"acSp", *b"inco", *b"mode",
                ],
                Object::Tab { .. } => &[*b"ID  ", *b"pnam", *b"URL ", *b"ldng", *b"loca"],
                Object::Space { .. } => &[*b"ID  ", *b"pnam"],
            };
            return Ok(Value::Record(
                code(b"reco"),
                keys.iter()
                    .map(|k| self.property(o, code(k)).map(|v| (code(k), v)))
                    .collect::<Result<_>>()?,
            ));
        }
        let text = |s: &str| Ok(Value::Text(s.into()));
        match o {
            Object::Application => {
                if p == code(b"pnam") {
                    text(&self.name)
                } else if p == code(b"vers") {
                    text(&self.version)
                } else if p == code(b"pisf") {
                    Ok(Value::Bool(self.frontmost))
                } else {
                    Err(Error::unsupported())
                }
            }
            Object::Window(id) => {
                let w = self.window(id)?;
                Ok(if p == code(b"ID  ") {
                    Value::Text(id.clone())
                } else if p == code(b"pnam") {
                    Value::Text(w.title.clone())
                } else if p == code(b"mode") {
                    Value::Text(w.mode.clone())
                } else if p == code(b"pidx") {
                    Value::Int(self.windows.iter().position(|w| &w.id == id).unwrap() as i64 + 1)
                } else if p == code(b"inco") {
                    Value::Bool(w.mode == "incognito")
                } else if p == code(b"acTa") {
                    w.active_tab
                        .as_ref()
                        .map(|tab| {
                            Value::Object(Object::Tab {
                                window: id.clone(),
                                id: tab.clone(),
                            })
                        })
                        .unwrap_or(Value::Null)
                } else if p == code(b"acSp") {
                    w.active_space
                        .as_ref()
                        .map(|space| {
                            Value::Object(Object::Space {
                                window: id.clone(),
                                id: space.clone(),
                            })
                        })
                        .unwrap_or(Value::Null)
                } else if p == code(b"pmnd") {
                    Value::Bool(w.minimized)
                } else if p == code(b"pvis") {
                    Value::Bool(w.visible)
                } else if p == code(b"pzum") {
                    Value::Bool(w.zoomed)
                } else if [code(b"hclb"), code(b"ismn"), code(b"prsz"), code(b"iszm")].contains(&p)
                {
                    Value::Bool(true)
                } else {
                    return Err(Error::unsupported());
                })
            }
            Object::Tab { window, id } => {
                let w = self.window(window)?;
                let t = w
                    .tabs
                    .iter()
                    .chain(w.spaces.iter().flat_map(|s| &s.tabs))
                    .find(|t| &t.id == id)
                    .ok_or_else(Error::missing)?;
                if p == code(b"ID  ") {
                    text(id)
                } else if p == code(b"pnam") {
                    text(&t.title)
                } else if p == code(b"URL ") {
                    text(&t.url)
                } else if p == code(b"loca") {
                    text(&t.location)
                } else if p == code(b"ldng") {
                    Ok(Value::Bool(t.loading))
                } else {
                    Err(Error::unsupported())
                }
            }
            Object::Space { window, id } => {
                let s = self
                    .window(window)?
                    .spaces
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or_else(Error::missing)?;
                if p == code(b"ID  ") {
                    text(id)
                } else if p == code(b"pnam") {
                    text(&s.title)
                } else {
                    Err(Error::unsupported())
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Snapshot {
        let tab = |id: &str, url: &str| Tab {
            id: id.into(),
            title: "Same title".into(),
            url: url.into(),
            loading: false,
            location: "unpinned".into(),
        };
        Snapshot {
            name: "Echo".into(),
            version: "1".into(),
            frontmost: true,
            windows: vec![
                Window {
                    id: "a".into(),
                    title: "A".into(),
                    mode: "normal".into(),
                    active_tab: Some("1".into()),
                    active_space: Some("s1".into()),
                    tabs: vec![tab("1", "https://first.example")],
                    spaces: vec![
                        Space {
                            id: "s1".into(),
                            title: "Work".into(),
                            tabs: vec![tab("1", "https://first.example")],
                        },
                        Space {
                            id: "s2".into(),
                            title: "Other".into(),
                            tabs: vec![tab("2", "https://other.example")],
                        },
                    ],
                    minimized: false,
                    visible: true,
                    zoomed: false,
                },
                Window {
                    id: "b".into(),
                    title: "B".into(),
                    mode: "incognito".into(),
                    active_tab: Some("1".into()),
                    active_space: None,
                    tabs: vec![tab("1", "https://private.example")],
                    spaces: vec![],
                    minimized: false,
                    visible: true,
                    zoomed: false,
                },
            ],
        }
    }
    #[test]
    fn dictionary_references_remain_scoped_when_tab_ids_overlap() {
        let state = fixture();
        for (w, url) in [
            ("a", "https://first.example"),
            ("b", "https://private.example"),
        ] {
            let q = Query::Property {
                property: code(b"URL "),
                container: Box::new(
                    Object::Tab {
                        window: w.into(),
                        id: "1".into(),
                    }
                    .query(),
                ),
            };
            assert_eq!(state.get(&q), Ok(Value::Text(url.into())))
        }
        let missing = Object::Tab {
            window: "b".into(),
            id: "2".into(),
        }
        .query();
        assert_eq!(state.one(&missing), Err(Error::missing()));
    }
    #[test]
    fn background_space_references_are_stable_and_every_preserves_list_shape() {
        let state = fixture();
        let q = Object::Tab {
            window: "a".into(),
            id: "2".into(),
        }
        .query();
        assert_eq!(
            state.get(&Query::Property {
                property: code(b"URL "),
                container: Box::new(q)
            }),
            Ok(Value::Text("https://other.example".into()))
        );
        let all = Query::Elements {
            class: code(b"tAbB"),
            container: Box::new(Object::Window("a".into()).query()),
            selector: Selector::All,
        };
        assert!(matches!(state.get(&all),Ok(Value::List(v)) if v.len()==1));
    }
    #[test]
    fn negative_indexes_and_missing_objects_never_fall_back_to_front_window() {
        let state = fixture();
        let window = |n| Query::Elements {
            class: code(b"WiND"),
            container: Box::new(Query::Application),
            selector: Selector::Index(n),
        };
        assert_eq!(state.one(&window(-1)), Ok(Object::Window("b".into())));
        for n in [0, 3, -3] {
            assert_eq!(state.one(&window(n)), Err(Error::missing()))
        }
        let old = Object::Window("gone".into()).query();
        assert_eq!(state.one(&old), Err(Error::missing()));
    }
    #[test]
    fn scripting_dictionary_commands_keep_exact_event_codes_and_reject_unknown_terms() {
        let params = Record::from([
            (
                code(b"----"),
                Value::Object(Object::Tab {
                    window: "a".into(),
                    id: "1".into(),
                }),
            ),
            (code(b"JvSc"), Value::Text("1+1".into())),
        ]);
        assert!(
            matches!(Command::decode(code(b"CrSu"),code(b"ExJa"),&params),Ok(Command::Execute(_,s)) if s=="1+1")
        );
        assert!(matches!(
            Command::decode(code(b"CrSe"), code(b"lect"), &params),
            Ok(Command::Select(_))
        ));
        assert!(Command::decode(code(b"CrSu"), code(b"xxxx"), &params).is_err());
    }
}
