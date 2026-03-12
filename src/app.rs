use leptos::{prelude::*, server};
use serde::{Deserialize, Serialize};
use chrono::{NaiveDateTime};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Comment {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub user_name: String,
    pub content: String,
    pub created_at: NaiveDateTime,
}

#[server]
pub async fn get_comments(page_id: String) -> Result<Vec<Comment>, ServerFnError> {
    println!("  get_comments: page_id={}", page_id);
    use sqlx::SqlitePool;
    let pool = expect_context::<SqlitePool>();
    let comments = sqlx::query_as::<_, Comment>(
        "SELECT id, parent_id, user_name, content, created_at
         FROM comments
         WHERE page_id = ?
         ORDER BY created_at ASC")
        .bind(page_id)
        .fetch_all(&pool)
        .await?;

    println!("  comments count={:?}", comments.len());
    Ok(comments)
}

#[server]
pub async fn add_comment(
    page_id: String,
    user_name: String,
    user_email: String,
    content: String,
    parent_id: Option<i64>,
) -> Result<(), ServerFnError> {
    use sqlx::SqlitePool;
    let pool = expect_context::<SqlitePool>();

    sqlx::query("INSERT INTO comments (page_id, user_name, user_email, content, parent_id, status) VALUES (?, ?, ?, ?, ?, 'pending')")
        .bind(page_id)
        .bind(user_name)
        .bind(user_email)
        .bind(content)
        .bind(parent_id)
        .execute(&pool)
        .await?;
    Ok(())
}

fn comment_thread(
    comment: Comment,
    all_comments: HashMap<Option<i64>, Vec<Comment>>,
    reply_to_signal: (ReadSignal<Option<(i64, String)>>, WriteSignal<Option<(i64, String)>>),
) -> AnyView {
    let (_, set_reply_to) = reply_to_signal; // 只需要 setter，但保留元组以便传递
    let children = all_comments.get(&Some(comment.id)).cloned().unwrap_or_default();

    // 构建子评论视图列表（递归调用自身，返回 Vec<View>）
    let children_views: Vec<AnyView> = children
        .into_iter()
        .map(|child| {
            comment_thread(child, all_comments.clone(), reply_to_signal)
        })
        .collect();

    view! {
      <div class="ml-4 mt-2 border-l-2 border-gray-200 pl-4">
        <div class="flex items-start">
          <div class="flex-1">
            <strong class="mr-2">{comment.user_name.clone()}</strong>
            <span class="text-gray-400 text-sm">
              {comment.created_at.and_utc().to_string()}
            </span>
          </div>
          <button
            type="button"
            class="text-blue-500 hover:text-blue-700 cursor-pointer"
            on:click=move |_| set_reply_to.set(Some((comment.id, comment.user_name.clone())))
          >
            "💬 回复"
          </button>
        </div>
        <p class="mt-1 text-gray-700">{comment.content.clone()}</p>
        {children_views}
      </div>
    }
    .into_any() // 将 HtmlElement<Div> 转换为 View
}

#[component]
pub fn CommentSystem(page_id: String) -> impl IntoView {
    let page_id = if page_id.is_empty() {
        let location = leptos::web_sys::window().unwrap().location();
        location.pathname().unwrap()
    } else {
        page_id
    };

    // (parent_id, parent_user_name)
    let (reply_to, set_reply_to) = signal(None::<(i64, String)>);

    let add_comment = ServerAction::<AddComment>::new();
    let page_id_cloned = page_id.clone();
    let comments_resource = Resource::new(
        move || (add_comment.version().get(), page_id_cloned.clone()),
        move |(_, pid)| get_comments(pid)
    );
    
    
    let result = view! {
      <div class="comment-container mt-4">
        // action ~ server_function; name attribute ~ arguments of server function
        <ActionForm action=add_comment>
          <input type="hidden" name="page_id" value=page_id />
          <input
            type="hidden"
            name="parent_id"
            value=move || { reply_to.get().map(|(id, _)| id.to_string()).unwrap_or_default() }
          />

          <div class="relative shrink w-full m-2 rounded-xl border border-solid border-gray-300 mb-5">

            // 显示当前回复目标
            {move || {
              reply_to
                .get()
                .map(|(_, name)| {
                  view! {
                    <div class="flex items-center justify-between bg-blue-50 px-3 py-2 text-sm text-blue-700 rounded-t-xl border-b border-blue-200">
                      <span>"回复 @" {name}</span>
                      <button
                        type="button"
                        class="text-blue-500 hover:text-blue-700 cursor-pointer"
                        on:click=move |_| set_reply_to.set(None)
                      >
                        "取消"
                      </button>
                    </div>
                  }
                })
            }}
            <div class="flex rounded-t-xl overflow-hidden px-1 border-b-2 border-dashed border-b-gray-300">
              <div class="flex flex-1">
                <label class="min-w-10 text-[#444] text-xs text-center py-3 px-2">昵称</label>
                <input
                  class="resize-none w-0 text-[0.625em] flex-1 p-2 bg-transparent"
                  type="text"
                  name="user_name"
                  required
                />
              </div>
              <div class="flex flex-1">
                <label class="min-w-10 text-[#444] text-xs text-center py-3 px-2">邮箱</label>
                <input
                  class="resize-none w-0 text-[0.625em] flex-1 p-2 bg-transparent"
                  type="email"
                  name="user_email"
                  required
                />
              </div>
            </div>
            <textarea
              // class="max-w-full text-[#444444] border-none outline-none transition-all duration-300"
              class="relative resize-y box-border w-[calc(100%-1em)] min-h-32 text-xs my-3 mx-2 rounded-xs bg-transparent"
              name="content"
              placeholder="欢迎评论"
              required
            ></textarea> <div class="relative flex flex-wrap mx-2 my-3">
              <div class="flex items-center justify-end flex-3 flex-shrink">
                <button
                  type="submit"
                  class="inline-block px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 cursor-pointer"
                >
                  {move || { if add_comment.pending().get() { "发送中..." } else { "提交" } }}
                </button>
              </div>
            </div>
          </div>

        </ActionForm>

        <h3>"评论"</h3>

        // <Suspense fallback=|| {
        // view! { <p>"加载中..."</p> }
        // }>
        // {move || {
        // comments
        // .get()
        // .map(|res| match res {
        // Ok(items) if !items.is_empty() => {
        // items
        // .into_iter()
        // .map(|c| {
        // let s = c.created_at.and_utc().date_naive().to_string();

        // view! {
        // <div class="m-3">
        // <strong class="mr-4">{c.user_name}</strong>
        // <span class="text-gray-400">{s}</span>
        // <button type="button" class="float-right cursor-pointer" title="回复">
        // "💬"
        // </button>
        // <p>{c.content}</p>
        // </div>
        // }
        // })
        // .collect_view()
        // .into_any()
        // }
        // _ => {
        // view! {
        // <p class="text-gray-400 text-center">"暂无评论，快来抢沙发！"</p>
        // }
        // .into_any()
        // }
        // })
        // }}
        // </Suspense>

        <Suspense fallback=|| {
          view! { <p>"加载中..."</p> }
        }>
          {move || {
            comments_resource
              .get()
              .map(|res| match res {
                Ok(comments) if !comments.is_empty() => {
                  let mut map: HashMap<Option<i64>, Vec<Comment>> = HashMap::new();
                  for comment in comments.clone() {
                    map.entry(comment.parent_id).or_default().push(comment);
                  }
                  if let Some(root_comments) = map.get_mut(&None) {
                    root_comments.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                  }
                  let root_comments = map.remove(&None).unwrap_or_default();
                  let root_views: Vec<AnyView> = root_comments
                    .into_iter()
                    .map(|root| comment_thread(root, map.clone(), (reply_to, set_reply_to)))
                    .collect();
                  if root_views.is_empty() {
                    // 构建评论树映射

                    view! {
                      <p class="text-gray-400 text-center">"暂无评论，快来抢沙发！"</p>
                    }
                      .into_any()
                  } else {
                    view! { {root_views} }.into_any()
                  }
                }
                _ => {
                  view! {
                    <p class="text-gray-400 text-center">"暂无评论，快来抢沙发！"</p>
                  }
                    .into_any()
                }
              })
          }}
        </Suspense>

      </div>
    };

    
    result
}

