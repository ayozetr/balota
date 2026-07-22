//! Minimal parser for Valve's KeyValues format (VDF).
//!
//! Used to read `libraryfolders.vdf` (Steam libraries spread across several
//! drives) and `appmanifest_221100.acf` (DayZ's real install folder, which is
//! not always called "DayZ").

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(entries) => entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Value::Str(_) => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Obj(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Obj(entries) => entries,
            Value::Str(_) => &[],
        }
    }
}

/// Parses a whole VDF document. The root object is implicit: a file like
/// `"libraryfolders" { ... }` comes back as an object with a single key.
pub fn parse(input: &str) -> Value {
    let tokens = tokenize(input);
    let mut pos = 0;
    Value::Obj(parse_entries(&tokens, &mut pos))
}

#[derive(Debug, PartialEq)]
enum Token {
    Str(String),
    Open,
    Close,
}

fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Line comments
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        match c {
            '{' => {
                tokens.push(Token::Open);
                i += 1;
            }
            '}' => {
                tokens.push(Token::Close);
                i += 1;
            }
            '"' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                        s.push(match chars[i] {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                i += 1; // closing quote
                tokens.push(Token::Str(s));
            }
            _ => {
                // Unquoted token: unusual in Steam's own files, but the format
                // allows it.
                let mut s = String::new();
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && chars[i] != '{'
                    && chars[i] != '}'
                {
                    s.push(chars[i]);
                    i += 1;
                }
                tokens.push(Token::Str(s));
            }
        }
    }

    tokens
}

fn parse_entries(tokens: &[Token], pos: &mut usize) -> Vec<(String, Value)> {
    let mut entries = Vec::new();

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Close => {
                *pos += 1;
                break;
            }
            Token::Open => {
                // Keyless block: shouldn't happen, skip it.
                *pos += 1;
                parse_entries(tokens, pos);
            }
            Token::Str(key) => {
                let key = key.clone();
                *pos += 1;
                match tokens.get(*pos) {
                    Some(Token::Open) => {
                        *pos += 1;
                        let inner = parse_entries(tokens, pos);
                        entries.push((key, Value::Obj(inner)));
                    }
                    Some(Token::Str(val)) => {
                        entries.push((key, Value::Str(val.clone())));
                        *pos += 1;
                    }
                    _ => break,
                }
            }
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_library_paths() {
        let src = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/home/user/.local/share/Steam"
		"label"		""
		"apps"
		{
			"221100"		"18374621"
		}
	}
	"1"
	{
		"path"		"/run/media/deck/SD/SteamLibrary"
		"apps"
		{
			"570"		"12"
		}
	}
}
"#;
        let root = parse(src);
        let folders = root.get("libraryfolders").unwrap();
        let paths: Vec<&str> = folders
            .entries()
            .iter()
            .filter_map(|(_, v)| v.get("path")?.as_str())
            .collect();

        assert_eq!(
            paths,
            vec![
                "/home/user/.local/share/Steam",
                "/run/media/deck/SD/SteamLibrary"
            ]
        );

        // The first library declares DayZ as installed, the second one doesn't.
        let apps = folders.get("0").unwrap().get("apps").unwrap();
        assert!(apps.get("221100").is_some());
        assert!(folders
            .get("1")
            .unwrap()
            .get("apps")
            .unwrap()
            .get("221100")
            .is_none());
    }

    #[test]
    fn reads_installdir_from_app_manifest() {
        let src = r#"
"AppState"
{
	"appid"		"221100"
	"name"		"DayZ"
	"installdir"		"DayZ"
	"UserConfig"
	{
		"language"		"english"
	}
}
"#;
        let root = parse(src);
        let install = root
            .get("AppState")
            .and_then(|v| v.get("installdir"))
            .and_then(|v| v.as_str());
        assert_eq!(install, Some("DayZ"));
    }

    #[test]
    fn handles_comments_and_escapes() {
        let src = r#"
// leading comment
"root"
{
	"path"	"C:\\games\\dayz"   // trailing comment
}
"#;
        let root = parse(src);
        assert_eq!(
            root.get("root").unwrap().get("path").unwrap().as_str(),
            Some(r"C:\games\dayz")
        );
    }
}
