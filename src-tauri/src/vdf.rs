// SPDX-License-Identifier: GPL-3.0-or-later
//! Minimal parser for Valve's KeyValues format (VDF).
//!
//! Used to read `libraryfolders.vdf` (Steam libraries spread across several
//! drives), `appmanifest_221100.acf` (DayZ's real install folder, which is not
//! always called "DayZ") and `loginusers.vdf` (the Steam display name offered
//! as the in-game name).

/// Deep enough for anything Steam writes, shallow enough that a corrupt file
/// cannot exhaust the stack. `parse_entries` recurses, and this runs on a
/// blocking pool thread whose stack overflow is a SIGSEGV, not a catchable
/// panic.
const MAX_DEPTH: usize = 64;

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
    // Steam writes these as plain UTF-8, but a file that has been through an
    // editor can pick up a BOM, and it is not whitespace: it would glue itself
    // to the first key and make every lookup miss.
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);

    let tokens = tokenize(input);
    let mut pos = 0;
    Value::Obj(parse_entries(&tokens, &mut pos, 0))
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
                // allows it. It has to stop at a quote as well, or a stray
                // character before a key swallows the key whole.
                let mut s = String::new();
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && chars[i] != '{'
                    && chars[i] != '}'
                    && chars[i] != '"'
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

fn parse_entries(tokens: &[Token], pos: &mut usize, depth: usize) -> Vec<(String, Value)> {
    let mut entries = Vec::new();
    if depth > MAX_DEPTH {
        // Skip to the end of this block rather than recursing further.
        let mut open = 1;
        while *pos < tokens.len() && open > 0 {
            match tokens[*pos] {
                Token::Open => open += 1,
                Token::Close => open -= 1,
                Token::Str(_) => {}
            }
            *pos += 1;
        }
        return entries;
    }

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Close => {
                *pos += 1;
                break;
            }
            Token::Open => {
                // Keyless block: shouldn't happen, skip it.
                *pos += 1;
                parse_entries(tokens, pos, depth + 1);
            }
            Token::Str(key) => {
                let key = key.clone();
                *pos += 1;
                match tokens.get(*pos) {
                    Some(Token::Open) => {
                        *pos += 1;
                        let inner = parse_entries(tokens, pos, depth + 1);
                        entries.push((key, Value::Obj(inner)));
                    }
                    Some(Token::Str(val)) => {
                        entries.push((key, Value::Str(val.clone())));
                        *pos += 1;
                    }
                    // A key with no value, right before the block ends. This
                    // used to `break` without consuming the `}`, which left the
                    // parent reading this block's closing brace as its own and
                    // silently dropped every sibling that followed.
                    Some(Token::Close) => {
                        *pos += 1;
                        break;
                    }
                    None => break,
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

    /// A file that has been through an editor can start with a BOM. It is not
    /// whitespace, so it used to be read as part of the first key and every
    /// library on the machine went missing.
    #[test]
    fn a_byte_order_mark_does_not_hide_the_first_key() {
        let src = "\u{FEFF}\"libraryfolders\"\n{\n\t\"0\" { \"path\" \"/lib\" }\n}\n";
        let root = parse(src);
        assert!(root.get("libraryfolders").is_some());
    }

    /// A key with no value used to desynchronise the parser and take every
    /// following sibling down with it.
    #[test]
    fn a_key_without_a_value_does_not_swallow_its_siblings() {
        let src = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"/first"
		"label"
	}
	"1"
	{
		"path"		"/second"
	}
}
"#;
        let folders = parse(src);
        let folders = folders.get("libraryfolders").unwrap();
        let paths: Vec<&str> = folders
            .entries()
            .iter()
            .filter_map(|(_, v)| v.get("path")?.as_str())
            .collect();
        assert_eq!(paths, vec!["/first", "/second"]);
    }

    /// Unbounded recursion here is a stack overflow, which `panic = "abort"`
    /// cannot even turn into an error message.
    #[test]
    fn deeply_nested_input_is_bounded_instead_of_overflowing_the_stack() {
        let deep = format!("\"a\"\n{}{}", "{ \"b\" ".repeat(20_000), "}".repeat(20_000));
        let parsed = parse(&deep);
        // The point is that it returns at all.
        assert!(parsed.get("a").is_some());
    }

    #[test]
    fn reads_the_most_recent_login() {
        let src = r#"
"users"
{
	"7656119"
	{
		"AccountName"		"ayoze"
		"PersonaName"		"Ayoze"
		"MostRecent"		"1"
	}
}
"#;
        let users = parse(src);
        let users = users.get("users").unwrap();
        let (_, user) = &users.entries()[0];
        assert_eq!(user.get("PersonaName").unwrap().as_str(), Some("Ayoze"));
        assert_eq!(user.get("mostrecent").unwrap().as_str(), Some("1"));
    }
}
