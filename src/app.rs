use leptos::{prelude::*, server};
use serde::{Deserialize, Serialize};
use chrono::{NaiveDateTime, FixedOffset};
use std::collections::HashMap;
use std::sync::Arc;

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
    use sqlx::SqlitePool;
    let pool = expect_context::<SqlitePool>();
    let comments = sqlx::query_as::<_, Comment>(
        "SELECT id, parent_id, user_name, content, created_at
         FROM comments
         WHERE page_id = ?
         -- WHERE page_id = ? AND status = 'approved'
         ORDER BY created_at ASC"
    )
    .bind(page_id)
    .fetch_all(&pool)
    .await?;
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
    sqlx::query(
        "INSERT INTO comments (page_id, user_name, user_email, content, parent_id, status) 
         VALUES (?, ?, ?, ?, ?, 'pending')"
    )
    .bind(page_id)
    .bind(user_name)
    .bind(user_email)
    .bind(content)
    .bind(parent_id)
    .execute(&pool)
    .await?;
    Ok(())
}

#[component]
fn InlineReplyForm(
    page_id: Arc<String>,
    parent_id: i64,
    add_comment: ServerAction<AddComment>,
    set_expanded: WriteSignal<Option<i64>>,
) -> impl IntoView {
    view! {
      <div class="mt-3 mb-2 border border-blue-200 rounded-lg p-3 bg-blue-50">
        <ActionForm action=add_comment>
          <input type="hidden" name="page_id" value=page_id.to_string() />
          <input type="hidden" name="parent_id" value=parent_id.to_string() />

          <div class="flex flex-col space-y-2">
            <div class="flex space-x-2">
              <input
                class="flex-1 p-2 text-xs border border-gray-300 rounded"
                type="text"
                name="user_name"
                placeholder="昵称"
                required
              />
              <input
                class="flex-1 p-2 text-xs border border-gray-300 rounded"
                type="email"
                name="user_email"
                placeholder="邮箱"
                required
              />
            </div>
            <textarea
              class="w-full p-2 text-xs border border-gray-300 rounded"
              name="content"
              placeholder="写下你的回复..."
              rows="2"
              required
            ></textarea>
            <div class="flex justify-end space-x-2">
              <button
                type="button"
                class="px-3 py-1 text-xs text-gray-600 border border-gray-300 rounded hover:bg-gray-100 cursor-pointer"
                on:click=move |_| set_expanded.set(None)
              >
                "取消"
              </button>
              <button
                type="submit"
                class="px-3 py-1 text-xs text-white bg-blue-500 rounded hover:bg-blue-600 disabled:bg-blue-300 cursor-pointer"
                disabled=move || add_comment.pending().get()
              >
                {move || { if add_comment.pending().get() { "发送中..." } else { "提交" } }}
              </button>
            </div>
          </div>
        </ActionForm>
      </div>
    }
}

fn comment_thread(
    comment: Comment,
    all_comments: HashMap<Option<i64>, Vec<Comment>>,
    expanded_reply: ReadSignal<Option<i64>>,
    set_expanded_reply: WriteSignal<Option<i64>>,
    add_comment: ServerAction<AddComment>,
    page_id: Arc<String>,
) -> AnyView {
    let children = all_comments.get(&Some(comment.id)).cloned().unwrap_or_default();

    let children_views: Vec<AnyView> = children
        .into_iter()
        .map(|child| {
            comment_thread(
                child,
                all_comments.clone(),
                expanded_reply,
                set_expanded_reply,
                add_comment.clone(),
                Arc::clone(&page_id),
            )
        })
        .collect();

    let comment_id = comment.id;
    let comment_user_name = comment.user_name.clone();
    let comment_content = comment.content.clone();
    let timezone_shanghai = FixedOffset::east_opt(8 * 3600).unwrap(); 
    let comment_date = comment.created_at.and_utc().with_timezone(&timezone_shanghai).to_rfc3339();

    let is_expanded = move || expanded_reply.get() == Some(comment_id);
    let on_reply_click = move |_| {
        if expanded_reply.get() == Some(comment_id) {
            set_expanded_reply.set(None);
        } else {
            set_expanded_reply.set(Some(comment_id));
        }
    };

    view! {
      <div class="ml-4 mt-2 border-l-2 border-gray-200 pl-4">
        <div class="flex items-start">
          <div class="flex-1">
            <strong class="mr-2">{comment_user_name.clone()}</strong>
            <span class="text-gray-400 text-sm">{comment_date.clone()}</span>
          </div>
          <button
            type="button"
            class="text-blue-500 hover:text-blue-700 cursor-pointer"
            title="回复"
            on:click=on_reply_click
          >
            "💬"
          </button>
        </div>
        <p class="mt-1 text-gray-700">{comment_content.clone()}</p>

        {move || {
          if is_expanded() {
            view! {
              <InlineReplyForm
                page_id=page_id.clone()
                parent_id=comment_id
                add_comment=add_comment.clone()
                set_expanded=set_expanded_reply
              />
            }
              .into_any()
          } else {
            ().into_any()
          }
        }}

        {children_views}
      </div>
    }
    .into_any()
}

#[component]
pub fn CommentSystem(page_id: String) -> impl IntoView {
    let page_id = if page_id.is_empty() {
        let location = leptos::web_sys::window().unwrap().location();
        location.pathname().unwrap()
    } else {
        page_id
    };
    let page_id_arc = Arc::new(page_id);

    let (expanded_reply, set_expanded_reply) = signal(None::<i64>);
    let add_comment = ServerAction::<AddComment>::new();
    let page_id_cloned = Arc::clone(&page_id_arc);
    let comments_resource = Resource::new(
        move || (add_comment.version().get(), page_id_cloned.clone()),
        move |(_, pid_arc)| get_comments((*pid_arc).clone())
    );

    Effect::new(move |_| {
        let _ = add_comment.version().get();
        set_expanded_reply.set(None);
    });

    let page_id_for_top_form = Arc::clone(&page_id_arc);
    let page_id_for_children = Arc::clone(&page_id_arc);

    view! {
      <div class="comment-container mt-4">
        <ActionForm action=add_comment>
          <input type="hidden" name="page_id" value=page_id_for_top_form.to_string() />
          <div class="relative shrink w-full m-2 rounded-xl border border-solid border-gray-300 mb-5">
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
              class="relative resize-y box-border w-[calc(100%-1em)] min-h-32 text-xs my-3 mx-2 rounded-xs bg-transparent"
              name="content"
              placeholder="欢迎评论 (评论列表中只展示昵称，邮箱仅用于后台审核、通知)"
              required
            ></textarea>

            <div class="relative flex flex-wrap mx-2 my-3">
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

        <div>
          <div class="text-xl">"评论"</div>
        </div>
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
                    .map(|root| {
                      comment_thread(
                        root,
                        map.clone(),
                        expanded_reply,
                        set_expanded_reply,
                        add_comment.clone(),
                        Arc::clone(&page_id_for_children),
                      )
                    })
                    .collect();
                  if root_views.is_empty() {
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
    }
}
