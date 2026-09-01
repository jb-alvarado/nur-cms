mod bindings {
    wit_bindgen::generate!({
        path: "../../wit/nur-cms-plugin",
        world: "cms-plugin",
    });
}

use bindings::{
    exports::nur::cms::http_handler::{Guest, PluginError, Request, Response},
    nur::cms::{
        configuration,
        database::{self, Statement, Value},
        mail::{self, ContentKind, Message},
        types::Header,
    },
};

struct EchoPlugin;

impl Guest for EchoPlugin {
    fn handle(request: Request) -> Result<Response, PluginError> {
        let body = match request.route_id.as_str() {
            "echo" => request.body,
            "author" | "editor" => {
                let roles = request
                    .identity
                    .map(|identity| identity.roles.join(","))
                    .unwrap_or_default();
                format!("authorized roles: {roles}").into_bytes()
            }
            "database" => database_example(request.body)?,
            "rollback" => rollback_example()?,
            "mail" => {
                let customer = String::from_utf8(request.body)
                    .map_err(|_| PluginError::BadRequest("expected a customer email address".into()))?;
                let customer = customer.trim();
                if customer.is_empty() {
                    return Err(PluginError::BadRequest(
                        "expected a customer email address".into(),
                    ));
                }
                let public_url = configuration::public_url().ok_or_else(|| {
                    PluginError::Failed("public CMS URL is not configured".into())
                })?;
                let confirmation_link = format!("{public_url}/orders/example/confirm");
                mail::send(&Message {
                    target: "contact".into(),
                    recipient: None,
                    name: "Echo plugin".into(),
                    reply_to: "echo@example.org".into(),
                    subject: Some("Message from the echo plugin".into()),
                    text: format!("A new example order is ready. The confirmation link is {confirmation_link}. This message demonstrates delivery to the fixed merchant recipients."),
                    content_kind: ContentKind::UserInput,
                })?;
                mail::send(&Message {
                    target: "contact".into(),
                    recipient: Some(customer.into()),
                    name: "Echo plugin".into(),
                    reply_to: "echo@example.org".into(),
                    subject: Some("Confirm your example order".into()),
                    text: format!("Please confirm your example order using {confirmation_link}. This second message is delivered only to the dynamic customer recipient."),
                    content_kind: ContentKind::UserInput,
                })?;
                format!("merchant and customer mail accepted; link: {confirmation_link}").into_bytes()
            }
            "root" => b"Hello from a nur-cms root plugin route".to_vec(),
            _ => return Err(PluginError::NotFound),
        };

        Ok(Response {
            status: 200,
            headers: vec![Header {
                name: "content-type".into(),
                value: "text/plain; charset=utf-8".into(),
            }],
            body,
        })
    }
}

fn database_example(body: Vec<u8>) -> Result<Vec<u8>, PluginError> {
    let message = String::from_utf8(body)
        .ok()
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "Hello from the database host".into());
    let inserted = database::execute(&Statement {
        sql: "INSERT INTO echo_messages (message) VALUES ($1) RETURNING id, message".into(),
        params: vec![Value::Text(message)],
    })?;
    let selected = database::execute(&Statement {
        sql: "SELECT id, message FROM echo_messages ORDER BY id DESC LIMIT 1".into(),
        params: Vec::new(),
    })?;
    Ok(format!("inserted: {inserted:?}\nselected: {selected:?}").into_bytes())
}

fn rollback_example() -> Result<Vec<u8>, PluginError> {
    let value = "rollback sentinel".to_string();
    let count = || {
        let result = database::execute(&Statement {
            sql: "SELECT count(message) FROM echo_messages WHERE message = $1".into(),
            params: vec![Value::Text(value.clone())],
        })?;
        match result.rows.as_slice() {
            [row] => match row.as_slice() {
                [Value::Integer(count)] => Ok(*count),
                _ => Err(PluginError::Failed("unexpected count result".into())),
            },
            _ => Err(PluginError::Failed("unexpected count result".into())),
        }
    };
    let count_before = count()?;
    let failed = database::transaction(&[
        Statement {
            sql: "INSERT INTO echo_messages (message) VALUES ($1)".into(),
            params: vec![Value::Text(value.clone())],
        },
        Statement {
            sql: "INSERT INTO missing_echo_table (message) VALUES ($1)".into(),
            params: vec![Value::Text(value.clone())],
        },
    ]);
    if failed.is_ok() {
        return Err(PluginError::Failed("transaction unexpectedly succeeded".into()));
    }
    if count()? != count_before {
        return Err(PluginError::Failed("transaction was not rolled back".into()));
    }
    Ok(b"transaction rolled back".to_vec())
}

bindings::export!(EchoPlugin with_types_in bindings);
