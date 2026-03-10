use leptos::{prelude::*, server};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))] 
pub struct Comment {
    pub email: String,
    pub content: String,
}

#[server]
pub async fn get_comments(page_id: String) -> Result<Vec<Comment>, ServerFnError> {
    println!("  get_comments: page_id={}", page_id);
    use sqlx::SqlitePool;
    let pool = expect_context::<SqlitePool>();
    let comments = sqlx::query_as::<_, Comment>("SELECT email, content from comments where page_id = ?")
        .bind(page_id)
        .fetch_all(&pool)
        .await?;
    Ok(comments)
}

#[server]
pub async fn add_comment(page_id: String, email: String, content: String) -> Result<(), ServerFnError> {
    use sqlx::SqlitePool;
    let pool = expect_context::<SqlitePool>();

    sqlx::query("INSERT INTO comments (page_id, email, content) VALUES (?, ?, ?)")
        .bind(page_id)
        .bind(email)
        .bind(content)
        .execute(&pool)
        .await?;
    Ok(())
}

#[component]
pub fn CommentSystem(page_id: String) -> impl IntoView {
    let add_comment = ServerAction::<AddComment>::new();
    let page_id_value = page_id.clone();
    let comments = Resource::new(
        move || (add_comment.version().get(), page_id_value.clone()),
        move |(_, pid)| get_comments(pid)
    );

    let result = view! {
        <div class="comment-container" style="border-top: 1px solid #ccc; padding: 20px;">
            <h3>"评论区"</h3>
            <Suspense fallback=|| {
                view! { <p>"加载中..."</p> }
            }>
                {move || {
                    comments
                        .get()
                        .map(|res| match res {
                            Ok(items) if !items.is_empty() => {
                                items
                                    .into_iter()
                                    .map(|c| {
                                        view! {
                                            <div style="margin-bottom: 10px;">
                                                <strong>{c.email}</strong>
                                                :
                                                <span>{c.content}</span>
                                            </div>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }
                            _ => view! { <p>"暂无评论，快来抢沙发！"</p> }.into_any(),
                        })
                }}
            </Suspense>
            <hr />
            <ActionForm action=add_comment>
                <input type="hidden" name="page_id" value=page_id />
                <div style="display: flex; flex-direction: column; gap: 10px; max-width: 400px;">
                    <input
                        type="email"
                        name="email"
                        placeholder="你的邮箱"
                        required
                        style="padding: 8px;"
                    />
                    <textarea
                        name="content"
                        placeholder="评论内容..."
                        required
                        style="padding: 8px; min-height: 80px;"
                    ></textarea>
                    <button type="submit" style="padding: 10px; cursor: pointer;">
                        {move || {
                            if add_comment.pending().get() {
                                "发送中..."
                            } else {
                                "提交评论"
                            }
                        }}
                    </button>
                </div>
            </ActionForm>
        </div>
    };

    
    result
}

