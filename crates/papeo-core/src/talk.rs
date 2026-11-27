//! Leitor do Nextcloud Talk (spec 29), parte pura: endereço do servidor,
//! leitura das respostas da API OCS do Talk (conversas, mensagens,
//! sugestões de menção), texto com os marcadores trocados e o que vira
//! aviso. A rede fica no app (`src-tauri/src/talk.rs`).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Conta do Talk ligada a um serviço Nextcloud. A senha de app fica no
/// cofre, nunca aqui.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TalkAccount {
    /// Endereço base do Nextcloud (`https://nuvem.exemplo.com[/pasta]`).
    pub server: String,
    /// Nome de login devolvido pelo Login Flow v2.
    pub user: String,
    /// Id do usuário (o da WebDAV e o `actorId` das mensagens).
    #[serde(default)]
    pub uid: String,
}

impl TalkAccount {
    /// Conferida ao ler o arquivo: servidor `https:` e campos curtos.
    pub fn sanitized(mut self) -> Option<Self> {
        self.server = talk_base(&self.server)?;
        self.user = clean(&self.user, 200)?;
        self.uid = if self.uid.trim().is_empty() {
            self.user.clone()
        } else {
            clean(&self.uid, 200)?
        };
        Some(self)
    }
}

fn clean(text: &str, max: usize) -> Option<String> {
    let t = text.trim();
    (!t.is_empty() && t.chars().count() <= max && !t.chars().any(char::is_control))
        .then(|| t.to_owned())
}

/// Endereço base do Nextcloud a partir do endereço do serviço (que pode
/// ser o do Talk, `…/index.php/apps/spreed/`). Só `https:`.
pub fn talk_base(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return None;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    let path = parsed.path();
    let base = ["/index.php", "/apps/", "/login", "/ocs/", "/remote.php"]
        .iter()
        .filter_map(|marker| path.find(marker))
        .min()
        .map_or(path, |at| &path[..at]);
    let mut out = parsed.clone();
    out.set_path(base.trim_end_matches('/'));
    out.set_query(None);
    out.set_fragment(None);
    Some(out.as_str().trim_end_matches('/').to_owned())
}

fn origin(url: &str) -> Option<url::Origin> {
    url::Url::parse(url).ok().map(|u| u.origin())
}

/// O Login Flow v2 só vale se a página de aprovação e o servidor devolvido
/// forem do mesmo endereço (esquema, máquina e porta) que a pessoa deu.
pub fn login_urls_ok(base: &str, login: &str, server: Option<&str>) -> bool {
    let Some(want) = origin(base) else {
        return false;
    };
    let same = |u: &str| origin(u).is_some_and(|o| o == want) && u.starts_with("https://");
    same(login) && server.is_none_or(same)
}

/// Token de conversa do Talk (vira pedaço de URL).
pub fn valid_token(token: &str) -> bool {
    (4..=64).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// Caminho de arquivo na pasta da pessoa (vira pedaço da URL da WebDAV).
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && !path.split('/').any(|p| p == ".." || p == ".")
}

/// `data` de uma resposta OCS (`{"ocs": {"meta": …, "data": …}}`).
pub fn ocs_data(body: &[u8]) -> Option<Value> {
    let mut v: Value = serde_json::from_slice(body).ok()?;
    Some(v.get_mut("ocs")?.get_mut("data")?.take())
}

fn s(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn n(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Conversas

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub author: String,
    pub text: String,
    pub own: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub token: String,
    pub name: String,
    /// 1 conversa a dois, 2 grupo, 3 pública, 4 novidades, 5 ex-conversa a
    /// dois, 6 anotações.
    pub kind: u8,
    pub unread: u32,
    pub mention: bool,
    pub favorite: bool,
    /// Segundos desde 1970.
    pub activity: i64,
    pub last: Option<Brief>,
    /// Id da última mensagem (para saber o que é novo).
    pub last_id: i64,
    pub read_only: bool,
    pub has_call: bool,
    /// Muda quando a foto muda (para o cache).
    pub avatar_version: String,
}

pub fn parse_rooms(data: &Value, uid: &str) -> Vec<Room> {
    let Some(list) = data.as_array() else {
        return Vec::new();
    };
    let mut rooms: Vec<Room> = list
        .iter()
        .filter_map(|r| {
            let token = s(r, "token");
            if !valid_token(&token) {
                return None;
            }
            let last = r.get("lastMessage").filter(|m| m.is_object());
            Some(Room {
                name: crate::clip(s(r, "displayName").trim(), 200),
                kind: u8::try_from(n(r, "type")).unwrap_or(0),
                unread: u32::try_from(n(r, "unreadMessages")).unwrap_or(0),
                mention: b(r, "unreadMention") || b(r, "unreadMentionDirect"),
                favorite: b(r, "isFavorite"),
                activity: n(r, "lastActivity"),
                last_id: last.map(|m| n(m, "id")).unwrap_or(0),
                last: last.map(|m| Brief {
                    author: crate::clip(s(m, "actorDisplayName").trim(), 100),
                    text: crate::clip(&render(m), 300),
                    own: is_own(m, uid),
                }),
                read_only: n(r, "readOnly") == 1,
                has_call: b(r, "hasCall"),
                avatar_version: crate::clip(&s(r, "avatarVersion"), 64),
                token,
            })
        })
        .collect();
    sort_rooms(&mut rooms);
    rooms
}

/// Não lidas e favoritas primeiro; depois a atividade mais recente.
pub fn sort_rooms(rooms: &mut [Room]) {
    rooms.sort_by(|a, b| {
        (b.unread > 0)
            .cmp(&(a.unread > 0))
            .then(b.favorite.cmp(&a.favorite))
            .then(b.activity.cmp(&a.activity))
    });
}

pub fn unread_total(rooms: &[Room]) -> u32 {
    rooms.iter().map(|r| r.unread).fold(0, u32::saturating_add)
}

/// Mensagem nova para avisar: conversa com não lidas cuja última mensagem
/// mudou desde a consulta anterior e não é da própria pessoa. A primeira
/// consulta (`seen` vazio) só guarda a base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fresh {
    pub token: String,
    pub room: String,
    pub author: String,
    pub text: String,
}

pub fn fresh_messages(seen: &mut HashMap<String, i64>, rooms: &[Room], first: bool) -> Vec<Fresh> {
    let mut out = Vec::new();
    for r in rooms {
        let before = seen.insert(r.token.clone(), r.last_id);
        if first || r.unread == 0 || r.last_id <= before.unwrap_or(0) {
            continue;
        }
        let Some(last) = r.last.as_ref().filter(|l| !l.own) else {
            continue;
        };
        out.push(Fresh {
            token: r.token.clone(),
            room: r.name.clone(),
            author: last.author.clone(),
            text: last.text.clone(),
        });
    }
    // Conversa que sumiu (saiu dela): esquece.
    seen.retain(|t, _| rooms.iter().any(|r| &r.token == t));
    out
}

// ---------------------------------------------------------------------------
// Mensagens

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub id: String,
    pub name: String,
    /// Caminho na pasta de quem lê (para baixar pela WebDAV).
    pub path: String,
    pub mime: String,
    pub size: u64,
    pub preview: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub id: i64,
    pub author: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reaction {
    pub emoji: String,
    pub count: u32,
    pub mine: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: i64,
    pub author: String,
    pub author_id: String,
    pub own: bool,
    pub time: i64,
    pub text: String,
    pub system: bool,
    pub files: Vec<FileRef>,
    pub parent: Option<Quote>,
    pub reactions: Vec<Reaction>,
    pub replyable: bool,
}

/// Mensagens de sistema que não aparecem (reações, apagadas, edições).
fn hidden_system(kind: &str) -> bool {
    kind.starts_with("reaction")
        || matches!(
            kind,
            "message_deleted" | "message_edited" | "history_cleared" | "poll_voted"
        )
}

fn is_own(m: &Value, uid: &str) -> bool {
    s(m, "actorType") == "users" && !uid.is_empty() && s(m, "actorId") == uid
}

/// Texto com os marcadores (`{actor}`, `{mention-user1}`, `{file}`)
/// trocados pelo nome; menção vira `@nome`.
pub fn render(m: &Value) -> String {
    let mut text = s(m, "message");
    let Some(params) = m.get("messageParameters").and_then(Value::as_object) else {
        return text;
    };
    for (key, p) in params {
        let name = s(p, "name");
        let shown = if key.starts_with("mention-") {
            format!("@{name}")
        } else {
            name
        };
        text = text.replace(&format!("{{{key}}}"), &shown);
    }
    text
}

fn files(m: &Value) -> Vec<FileRef> {
    let Some(params) = m.get("messageParameters").and_then(Value::as_object) else {
        return Vec::new();
    };
    params
        .values()
        .filter(|p| s(p, "type") == "file")
        .map(|p| FileRef {
            id: crate::clip(&s(p, "id"), 64),
            name: crate::clip(s(p, "name").trim(), 200),
            path: s(p, "path"),
            mime: crate::clip(&s(p, "mimetype"), 100).to_ascii_lowercase(),
            size: p
                .get("size")
                .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
                .unwrap_or(0),
            preview: matches!(p.get("preview-available"), Some(Value::String(v)) if v == "yes")
                || b(p, "preview-available"),
        })
        .filter(|f| valid_path(&f.path))
        .collect()
}

fn message(m: &Value, uid: &str) -> Option<Message> {
    let system = s(m, "systemMessage");
    if hidden_system(&system) || s(m, "messageType") == "comment_deleted" {
        return None;
    }
    let mine: Vec<String> = m
        .get("reactionsSelf")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut reactions: Vec<Reaction> = m
        .get("reactions")
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .filter(|(e, _)| e.chars().count() <= 8)
                .map(|(e, c)| Reaction {
                    emoji: e.clone(),
                    count: c.as_u64().and_then(|c| u32::try_from(c).ok()).unwrap_or(0),
                    mine: mine.contains(e),
                })
                .filter(|r| r.count > 0)
                .collect()
        })
        .unwrap_or_default();
    reactions.sort_by(|a, b| b.count.cmp(&a.count).then(a.emoji.cmp(&b.emoji)));
    let parent = m.get("parent").filter(|p| p.is_object()).map(|p| Quote {
        id: n(p, "id"),
        author: crate::clip(s(p, "actorDisplayName").trim(), 100),
        text: crate::clip(&render(p), 300),
    });
    Some(Message {
        id: n(m, "id"),
        author: crate::clip(s(m, "actorDisplayName").trim(), 100),
        author_id: crate::clip(&s(m, "actorId"), 200),
        own: is_own(m, uid),
        time: n(m, "timestamp"),
        text: crate::clip(&render(m), 32_000),
        system: !system.is_empty(),
        files: files(m),
        parent,
        reactions,
        replyable: b(m, "isReplyable"),
    })
}

/// Mensagens em ordem de envio (a API devolve as antigas ao contrário).
pub fn parse_messages(data: &Value, uid: &str) -> Vec<Message> {
    let Some(list) = data.as_array() else {
        return Vec::new();
    };
    let mut out: Vec<Message> = list.iter().filter_map(|m| message(m, uid)).collect();
    out.sort_by_key(|m| m.id);
    out.dedup_by_key(|m| m.id);
    out
}

/// Maior id entre as mensagens da resposta, mesmo as escondidas (para a
/// espera seguinte não voltar a elas).
pub fn last_id(data: &Value) -> i64 {
    data.as_array()
        .map(|a| a.iter().map(|m| n(m, "id")).max().unwrap_or(0))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mention {
    /// O que vai no texto depois do `@` (entre aspas se tiver espaço).
    pub insert: String,
    pub label: String,
}

pub fn parse_mentions(data: &Value) -> Vec<Mention> {
    let Some(list) = data.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|m| {
            let id = s(m, "id");
            if id.is_empty() || id.chars().any(char::is_control) || id.contains('"') {
                return None;
            }
            let insert = if id.contains([' ', '@', '/']) || s(m, "source") != "users" {
                format!("\"{id}\"")
            } else {
                id.clone()
            };
            Some(Mention {
                insert,
                label: crate::clip(s(m, "label").trim(), 100),
            })
        })
        .take(10)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_talk_base() {
        assert_eq!(
            talk_base("https://nuvem.x.com/index.php/apps/spreed/").as_deref(),
            Some("https://nuvem.x.com")
        );
        assert_eq!(
            talk_base("https://x.com/nextcloud/apps/spreed/call/abc").as_deref(),
            Some("https://x.com/nextcloud")
        );
        assert_eq!(
            talk_base("https://x.com:8443/").as_deref(),
            Some("https://x.com:8443")
        );
        assert_eq!(talk_base("http://x.com/"), None);
        assert_eq!(talk_base("https://u:p@x.com/"), None);
        assert_eq!(talk_base("lixo"), None);
    }

    #[test]
    fn login_urls_must_match() {
        let base = "https://nuvem.x.com";
        assert!(login_urls_ok(
            base,
            "https://nuvem.x.com/login/v2/flow/abc",
            Some("https://nuvem.x.com")
        ));
        assert!(!login_urls_ok(base, "https://outro.com/login", None));
        assert!(!login_urls_ok(
            base,
            "https://nuvem.x.com/login",
            Some("https://nuvem.x.com:444")
        ));
        assert!(!login_urls_ok(base, "http://nuvem.x.com/login", None));
    }

    #[test]
    fn checks_tokens_and_paths() {
        assert!(valid_token("abc123xy"));
        assert!(!valid_token("ab/c"));
        assert!(!valid_token("../x"));
        assert!(valid_path("Talk/foto.png"));
        assert!(!valid_path("Talk/../x"));
        assert!(!valid_path(""));
    }

    #[test]
    fn account_is_sanitized() {
        let a = TalkAccount {
            server: "https://x.com/index.php/apps/spreed/".into(),
            user: " ana ".into(),
            uid: String::new(),
        }
        .sanitized()
        .unwrap();
        assert_eq!(a.server, "https://x.com");
        assert_eq!(a.uid, "ana");
        let bad = TalkAccount {
            server: "http://x.com".into(),
            user: "a".into(),
            uid: "a".into(),
        };
        assert!(bad.sanitized().is_none());
    }

    #[test]
    fn reads_rooms() {
        let body = json!({"ocs": {"meta": {"status": "ok"}, "data": [
            {"token": "aaaa1111", "type": 2, "displayName": "Equipe", "unreadMessages": 0,
             "lastActivity": 100, "isFavorite": true, "lastMessage": []},
            {"token": "bbbb2222", "type": 1, "displayName": "Bia", "unreadMessages": 2,
             "unreadMention": true, "lastActivity": 50,
             "lastMessage": {"id": 9, "actorType": "users", "actorId": "bia",
               "actorDisplayName": "Bia", "message": "oi {mention-user1}",
               "messageParameters": {"mention-user1": {"type": "user", "id": "ana", "name": "Ana"}}}},
            {"token": "x/y", "displayName": "inválida"}
        ]}});
        let data = ocs_data(body.to_string().as_bytes()).unwrap();
        let rooms = parse_rooms(&data, "ana");
        assert_eq!(rooms.len(), 2);
        assert_eq!(rooms[0].token, "bbbb2222");
        assert!(rooms[0].mention);
        assert_eq!(rooms[0].last.as_ref().unwrap().text, "oi @Ana");
        assert!(!rooms[0].last.as_ref().unwrap().own);
        assert!(rooms[1].last.is_none());
        assert_eq!(unread_total(&rooms), 2);
    }

    #[test]
    fn announces_only_new_messages_from_others() {
        let room = |id: i64, unread: u32, own: bool| Room {
            token: "aaaa1111".into(),
            name: "Equipe".into(),
            unread,
            last_id: id,
            last: Some(Brief {
                author: "Bia".into(),
                text: "oi".into(),
                own,
            }),
            ..Default::default()
        };
        let mut seen = HashMap::new();
        assert!(fresh_messages(&mut seen, &[room(5, 1, false)], true).is_empty());
        assert!(fresh_messages(&mut seen, &[room(5, 1, false)], false).is_empty());
        let fresh = fresh_messages(&mut seen, &[room(6, 2, false)], false);
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].room, "Equipe");
        assert!(fresh_messages(&mut seen, &[room(7, 0, false)], false).is_empty());
        assert!(fresh_messages(&mut seen, &[room(8, 1, true)], false).is_empty());
        assert!(fresh_messages(&mut seen, &[], false).is_empty());
        assert!(seen.is_empty());
    }

    #[test]
    fn reads_messages() {
        let data = json!([
            {"id": 12, "actorType": "users", "actorId": "ana", "actorDisplayName": "Ana",
             "timestamp": 200, "message": "{file}", "messageType": "comment", "systemMessage": "",
             "isReplyable": true,
             "messageParameters": {"file": {"type": "file", "id": "77", "name": "foto.png",
                "path": "Talk/foto.png", "mimetype": "image/png", "size": 1234,
                "preview-available": "yes"}},
             "reactions": {"👍": 2}, "reactionsSelf": ["👍"],
             "parent": {"id": 10, "actorDisplayName": "Bia", "message": "manda a foto"}},
            {"id": 11, "actorType": "users", "actorId": "bia", "actorDisplayName": "Bia",
             "timestamp": 150, "message": "{actor} reagiu", "systemMessage": "reaction",
             "messageParameters": []},
            {"id": 10, "actorType": "users", "actorId": "bia", "actorDisplayName": "Bia",
             "timestamp": 100, "message": "manda a foto", "systemMessage": "",
             "messageParameters": []},
            {"id": 9, "actorType": "users", "actorId": "bia", "actorDisplayName": "Bia",
             "timestamp": 90, "message": "{actor} adicionou {user}", "systemMessage": "user_added",
             "messageParameters": {"actor": {"type": "user", "name": "Bia"},
                                   "user": {"type": "user", "name": "Ana"}}}
        ]);
        let list = parse_messages(&data, "ana");
        assert_eq!(
            list.iter().map(|m| m.id).collect::<Vec<_>>(),
            vec![9, 10, 12]
        );
        assert!(list[0].system);
        assert_eq!(list[0].text, "Bia adicionou Ana");
        let photo = &list[2];
        assert!(photo.own);
        assert_eq!(photo.text, "foto.png");
        assert_eq!(photo.files[0].path, "Talk/foto.png");
        assert!(photo.files[0].preview);
        assert_eq!(photo.reactions[0].emoji, "👍");
        assert!(photo.reactions[0].mine);
        assert_eq!(photo.parent.as_ref().unwrap().author, "Bia");
        assert_eq!(last_id(&data), 12);
    }

    #[test]
    fn reads_mentions() {
        let data = json!([
            {"id": "ana", "label": "Ana", "source": "users"},
            {"id": "joão silva", "label": "João", "source": "users"},
            {"id": "all", "label": "Todos", "source": "calls"},
            {"id": "x\"y", "label": "ruim", "source": "users"}
        ]);
        let list = parse_mentions(&data);
        assert_eq!(
            list.iter().map(|m| m.insert.as_str()).collect::<Vec<_>>(),
            vec!["ana", "\"joão silva\"", "\"all\""]
        );
    }
}
