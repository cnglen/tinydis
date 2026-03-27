rust_i18n::i18n!();

pub mod app;

fn strip_protocol(url: &str) -> &str {
    url.strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url)
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use http::header::CONTENT_TYPE;
    use http::Method;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{handle_server_fns_with_context, LeptosRoutes};
    use sqlx::SqlitePool;
    use std::env;
    use tower_http::cors::{AllowOrigin, CorsLayer};

    // check config of env
    env::var("TINYDIS_SMTP_HOST").expect("TINYDIS_SMTP_HOST must be set");
    env::var("TINYDIS_SMTP_PORT").expect("TINYDIS_SMTP_PORT must be set");
    env::var("TINYDIS_SMTP_USERNAME").expect("TINYDIS_SMTP_USERNAME must be set");
    env::var("TINYDIS_SMTP_PASSWORD").expect("TINYDIS_SMTP_PASSWORD must be set");
    env::var("TINYDIS_ADMIN_EMAIL").expect("TINYDIS_ADMIN_EMAIL must be set");
    env::var("TINYDIS_ALLOWED_ORIGINS").expect("TINYDIS_ALLOWED_ORIGINS must be set");

    let locale = env::var("TINYDIS_ADMIN_LOCALE").unwrap_or(String::from("en"));
    log!("Admin mail locale: {}", locale);
    rust_i18n::set_locale(&locale);

    fn build_cors() -> CorsLayer {
        let origins_str =
            env::var("TINYDIS_ALLOWED_ORIGINS").expect("TINYDIS_ALLOWED_ORIGINS must be set");

        let origins = if origins_str == "*" {
            AllowOrigin::any()
        } else {
            origins_str
                .split(',')
                .map(|s| s.trim().parse().expect("Invalid origin"))
                .collect::<Vec<_>>()
                .into()
        };
        log!("Allowed origins: {:?}", origins);

        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([CONTENT_TYPE])
    }

    let pool = SqlitePool::connect("sqlite:comments.db?mode=rwc")
        .await
        .unwrap();
    let pool_ = pool.clone();
    sqlx::query("BEGIN;
CREATE TABLE IF NOT EXISTS comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT, -- unique id
    parent_id INTEGER,                    -- parent id, null if current comment is root comment
    page_id TEXT NOT NULL,                -- id of page,
    user_name TEXT NOT NULL,              -- user name
    content TEXT NOT NULL,                -- content of comment
    status TEXT NOT NULL DEFAULT 'pending', -- status: pending/approved/rejected
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (parent_id) REFERENCES comments(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_comments_pageid_status_parentid_createdat ON comments(page_id, status, parent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS review_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    comment_id INTEGER NOT NULL,
    token TEXT NOT NULL UNIQUE,
    expires_at DATETIME NOT NULL,
    FOREIGN KEY (comment_id) REFERENCES comments(id) ON DELETE CASCADE
);

COMMIT;")
        .execute(&pool).await.unwrap();

    let mut conf = get_configuration(None).unwrap();
    let addr_str = env::var("TINYDIS_SERVER_ADDR").unwrap_or("http://127.0.0.1:3000".to_string());
    let addr_str = strip_protocol(&addr_str).to_string();
    let addr: std::net::SocketAddr = addr_str.parse().expect("Invalid socket address format");
    conf.leptos_options.site_addr = addr;

    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // log!("{:#?}", leptos_options);

    let server_fn_handler = |req| {
        handle_server_fns_with_context(
            move || {
                provide_context(pool.clone());
            },
            req,
        )
    };

    use leptos_axum::generate_route_list;
    use tinydis::app::App;
    let routes = generate_route_list(App); // "/review-result" in App()
                                           // log!("routes by generate_route_list(App): {:#?}", routes);

    let app = Router::new()
        .route(
            "/api/review/{*fn_name}",
            axum::routing::get(server_fn_handler.clone()),
        )
        .route("/api/{*fn_name}", axum::routing::post(server_fn_handler))
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            move || provide_context(pool_.clone()),
            App,
        )
        // .layer(CorsLayer::permissive()) // for debug
        .layer(build_cors())
        .with_state(leptos_options);

    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
