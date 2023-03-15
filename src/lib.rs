use serde_json::json;
use worker::*;

mod utils;

fn log_request(req: &Request) {
    console_log!(
        "{} - [{}], located at: {:?}, within: {}",
        Date::now().to_string(),
        req.path(),
        req.cf().coordinates().unwrap_or_default(),
        req.cf().region().unwrap_or("unknown region".into())
    );
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    log_request(&req);

    // Optionally, get more helpful error messages written to the console in the case of a panic.
    utils::set_panic_hook();

    // Optionally, use the Router to handle matching endpoints, use ":name" placeholders, or "*name"
    // catch-alls to match on specific patterns. Alternatively, use `Router::with_data(D)` to
    // provide arbitrary data that will be accessible in each route via the `ctx.data()` method.
    let router = Router::new();

    // Add as many routes as your Worker needs! Each route will get a `Request` for handling HTTP
    // functionality and a `RouteContext` which you can use to  and get route parameters and
    // Environment bindings like KV Stores, Durable Objects, Secrets, and Variables.
    router
        .get_async("/", |_, ctx| async move {
            use libsql_client::{workers::Connection as DbConnection, Connection, ResultSet};
            use std::convert::TryFrom;

            // Set up the database connection. connect_from_ctx assumes that you put the following variables
            // in .dev.vars or Cloudflare Workers secrets (wrangler secret put (...)):
            //   LIBSQL_CLIENT_URL=https://your-db-name.turso.io # remember about https:// or http://! It needs to be specified
            //   LIBSQL_CLIENT_TOKEN=<your token obtained via `turso db auth token` command>
            let connection = match DbConnection::connect_from_ctx(&ctx) {
                Ok(conn) => conn,
                Err(e) => return Response::error(e.to_string(), 400),
            };

            // Execute a database query - select a random number. We're not inserting anything
            // to the database this time, but execute() can work that way too, just send an INSERT or UPDATE statement.
            let result_set = match connection.execute("SELECT random() AS lucky_number").await {
                Ok(result) => result.into_result_set(),
                Err(e) => return Response::error(e.to_string(), 400),
            };

            // Extract the query result - we're expecting a single number to be returned under the column "lucky_number"
            let lucky_number = match result_set {
                Ok(ResultSet { columns: _, rows }) => {
                    let value = &rows.first().expect("expected one row").cells["lucky_number"];
                    i64::try_from(value.clone()).unwrap_or_default()
                }

                Err(e) => return Response::error(e.to_string(), 400),
            };

            // Generate the successful response via Workers API
            Response::ok(format!("Your lucky number is: {lucky_number}"))
        })
        .post_async("/form/:field", |mut req, ctx| async move {
            if let Some(name) = ctx.param("field") {
                let form = req.form_data().await?;
                match form.get(name) {
                    Some(FormEntry::Field(value)) => {
                        return Response::from_json(&json!({ name: value }))
                    }
                    Some(FormEntry::File(_)) => {
                        return Response::error("`field` param in form shouldn't be a File", 422);
                    }
                    None => return Response::error("Bad Request", 400),
                }
            }

            Response::error("Bad Request", 400)
        })
        .get("/worker-version", |_, ctx| {
            let version = ctx.var("WORKERS_RS_VERSION")?.to_string();
            Response::ok(version)
        })
        .run(req, env)
        .await
}
