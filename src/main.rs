pub mod app;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use leptos::logging::log;
    use leptos::prelude::*;
    use sqlx::SqlitePool;
    use tower_http::cors::CorsLayer;
    use leptos_axum::handle_server_fns_with_context;
    
    let pool = SqlitePool::connect("sqlite:comments.db?mode=rwc").await.unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS comments (id INTEGER PRIMARY KEY, page_id TEXT, email TEXT, content TEXT)")
        .execute(&pool).await.unwrap();
    
    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;

    let server_fn_handler = |req| {
        handle_server_fns_with_context(
            move || { provide_context(pool.clone());},
            req
        )
    };
    
    let app = Router::new()
        .route(
            "/api/{*fn_name}",
            axum::routing::post( server_fn_handler ),
        )
        .layer(CorsLayer::permissive())
        .with_state(leptos_options)
        ;

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
