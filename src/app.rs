use chrono::{FixedOffset, NaiveDateTime};
use leptos::server_fn::codec::GetUrl;
use leptos::{prelude::*, server};
use leptos_router::components::{Route, Router, Routes};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
// use leptos::logging::log;
include!(concat!(env!("OUT_DIR"), "/i18n/mod.rs"));
use i18n::*;

#[derive(Clone, Default)]
struct ReplyDraft {
    user_name: String,
    content: String,
}

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
         WHERE page_id = ? AND status = 'approved'
         ORDER BY created_at ASC",
    )
    .bind(page_id)
    .fetch_all(&pool)
    .await?;

    Ok(comments)
}

#[cfg(feature = "ssr")]
fn get_mail_config() -> (String, String, String, String, String) {
    (
        std::env::var("TINYDIS_SMTP_HOST").expect("TINYDIS_SMTP_HOST must be set"),
        std::env::var("TINYDIS_SMTP_PORT").expect("TINYDIS_SMTP_PORT must be set"),
        std::env::var("TINYDIS_SMTP_USERNAME").expect("TINYDIS_SMTP_USERNAME must be set"),
        std::env::var("TINYDIS_SMTP_PASSWORD").expect("TINYDIS_SMTP_PASSWORD must be set"),
        std::env::var("TINYDIS_ADMIN_EMAIL").expect("TINYDIS_ADMIN_EMAIL must be set"),
    )
}

#[cfg(feature = "ssr")]
async fn send_email(
    to: &str,
    subject: &str,
    body: &str,
    content_type: &str,
) -> Result<(), MailError> {
    use lettre::{
        message::header, message::Mailbox, transport::smtp::authentication::Credentials,
        AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    };
    let (host, port, username, password, _) = get_mail_config();
    let from: Mailbox = username
        .parse()
        .map_err(|e: lettre::address::AddressError| {
            MailError::InvalidFromEmail(format!("{e}: {username}"))
        })?;
    let to: Mailbox = to
        .parse()
        .map_err(|e| MailError::InvalidFromEmail(format!("{e}: {to}")))?;

    let content_type = match content_type {
        "html" => header::ContentType::TEXT_HTML,
        "plain" => header::ContentType::TEXT_PLAIN,
        &_ => todo!(),
    };

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject(subject)
        .header(content_type)
        .body(body.to_string())
        .map_err(|e| MailError::BuildMessage(format!("Failed to build email: {}", e)))?;

    let port: u16 = port
        .parse()
        .map_err(|e| MailError::InvalidPort(format!("{e} {port}")))?;

    let creds = Credentials::new(username, password);
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&host)
        .map_err(|e| MailError::SmtpRelay(format!("SMTP relay error: {}", e)))?
        .port(port)
        .credentials(creds)
        .build();

    mailer
        .send(email)
        .await
        .map_err(|e| MailError::SendMail(format!("Failed to send email: {}", e)))?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum MailError {
    #[error("Invalid email(from): {0}")]
    InvalidFromEmail(String),
    #[error("Invalid email(to): {0}")]
    InvalidToEmail(String),
    #[error("Unable to build message: {0}")]
    BuildMessage(String),

    #[error("Smtp relay error: {0}")]
    SmtpRelay(String),

    #[error("Unable to send mail: {0}")]
    SendMail(String),

    #[error("Invalid port: {0}")]
    InvalidPort(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AddCommentResponse {
    form_id: String,
    parent_id: Option<i64>,
}

/// # Arguments
/// - `page_id`: page id
/// - `user_name`: user name
/// - `content`: content
/// - `parent_id`: parent id
/// - `form_id`: "main"/"inline"
///
/// # Returns
/// A `AddCommentResponse`, where `parent_id` is used to update reply draft and `form_id` is used to show_main_form
#[server]
pub async fn add_comment(
    page_id: String,
    user_name: String,
    content: String,
    parent_id: Option<i64>,
    form_id: String,
) -> Result<AddCommentResponse, ServerFnError> {
    // log!("add comment ...");
    use chrono::{Duration, Utc};
    use leptos::server_fn::ServerFn;
    use sqlx::SqlitePool;
    use uuid::Uuid;
    use leptos_axum::extract;
    use axum::http::HeaderMap;

    let headers: HeaderMap = extract().await?;
    let host = headers.get("host").expect("host not found").to_str().expect("host to str failed");
    
    let pool = expect_context::<SqlitePool>();
    let result = sqlx::query(
        "INSERT INTO comments (page_id, user_name, content, parent_id, status) 
         VALUES (?, ?, ?, ?, 'pending')",
    )
    .bind(page_id)
    .bind(user_name.clone())
    .bind(content.clone())
    .bind(parent_id)
    .execute(&pool)
    .await?;
    let comment_id = result.last_insert_rowid();

    let token = Uuid::new_v4().to_string();
    let expires_at = Utc::now() + Duration::days(7); // 有效期7天
    sqlx::query("INSERT INTO review_tokens (comment_id, token, expires_at) VALUES (?, ?, ?)")
        .bind(comment_id)
        .bind(&token)
        .bind(expires_at.naive_utc())
        .execute(&pool)
        .await?;

    let (_, _, _, _, admin_email) = get_mail_config();
    // let base_url = std::env::var("TINYDIS_SERVER_ADDR")
    //     .expect("TINYDIS_SERVER_ADDR must be set such as http://your_domain_or_ip:your_port");
    let approve_link = format!("http://{}{}?token={}", host, ApproveComment::url(), token);
    let reject_link = format!("http://{}{}?token={}", host, RejectComment::url(), token);

    let email_body = format!(
        r#"<p>{}: </p>
         <p>{}: {}</p>
         <p>{}: {}</p>
         <p>
           <ul>
             <li><a href="{}" style="color: green">{}</a></li>
             <li><a href="{}" style="color: red">{}</a></li>
           </ul>
         </p>
         <p>{}</p>"#,
        rust_i18n::t!("newcomment_need_review"),
        rust_i18n::t!("user_name"),
        user_name,
        rust_i18n::t!("content"),
        content,
        approve_link,
        rust_i18n::t!("approve"),
        reject_link,
        rust_i18n::t!("reject"),
        rust_i18n::t!("link_valid_for_7_days")
    );
    let email_title = rust_i18n::t!("newcomment_need_review");

    tokio::spawn(async move {
        if let Err(e) = send_email(&admin_email, &email_title, &email_body, "html").await {
            eprintln!("Failed to send admin email: {}", e);
        }
    });

    Ok(AddCommentResponse { form_id, parent_id })
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
struct ReviewToken {
    id: i64,
    comment_id: i64,
    token: String,
    expires_at: NaiveDateTime,
}

#[server(endpoint="review/approve_comment", input=GetUrl)]
pub async fn approve_comment(token: String) -> Result<(), ServerFnError> {
    use chrono::Utc;
    use sqlx::SqlitePool;

    let pool = expect_context::<SqlitePool>();

    let token_record: Option<ReviewToken> = sqlx::query_as(
        "SELECT id, comment_id, token, expires_at
         FROM review_tokens
         WHERE token = ?",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await?;

    let result = match token_record {
        Some(r) if r.expires_at > Utc::now().naive_utc() => {
            sqlx::query(
                "UPDATE comments
         SET status='approved'
         WHERE id = ?",
            )
            .bind(r.comment_id)
            .execute(&pool)
            .await?;

            sqlx::query("DELETE FROM review_tokens WHERE id=?")
                .bind(r.id)
                .execute(&pool)
                .await?;
            "approved"
        }
        Some(_) => "expired",
        None => "invalid",
    };

    let redirect_url = format!("/review-result?result={}", result);
    leptos_axum::redirect(&redirect_url);
    Ok(())
}

#[cfg(feature = "ssr")]
#[server(endpoint="review/reject_comment", input=GetUrl)]
pub async fn reject_comment(token: String) -> Result<(), ServerFnError> {
    use chrono::Utc;
    use sqlx::SqlitePool;

    let pool = expect_context::<SqlitePool>();

    let token_record: Option<ReviewToken> = sqlx::query_as(
        "SELECT id, comment_id, token, expires_at
         FROM review_tokens
         WHERE token = ?",
    )
    .bind(&token)
    .fetch_optional(&pool)
    .await?;

    let result = match token_record {
        Some(r) if r.expires_at > Utc::now().naive_utc() => {
            sqlx::query(
                "UPDATE comments
         SET status='rejected'
         WHERE id = ?",
            )
            .bind(r.comment_id)
            .execute(&pool)
            .await?;

            sqlx::query("DELETE FROM review_tokens WHERE id=?")
                .bind(r.id)
                .execute(&pool)
                .await?;

            "rejected"
        }
        Some(_) => "expired",
        None => "invalid",
    };

    let redirect_url = format!("/review-result?result={}", result);
    leptos_axum::redirect(&redirect_url);
    Ok(())
}

#[component]
fn InlineReplyForm(
    page_id: Arc<String>,
    parent_id: i64,
    add_comment: ServerAction<AddComment>,
    active_reply_parent_id: ReadSignal<Option<i64>>,
    set_active_reply_parent_id: WriteSignal<Option<i64>>,
    draft: Signal<ReplyDraft>,
    set_draft: Callback<ReplyDraft>,
) -> impl IntoView {
    let i18n = use_i18n();
    let user_name = Signal::derive(move || draft.get().user_name);
    let set_user_name = Callback::new(move |new_name: String| {
        let current = draft.get_untracked();
        set_draft.run(ReplyDraft {
            user_name: new_name,
            content: current.content,
        });
    });

    let content = Signal::derive(move || draft.get().content);
    let set_content = Callback::new(move |new_content: String| {
        let current = draft.get_untracked();
        set_draft.run(ReplyDraft {
            user_name: current.user_name,
            content: new_content,
        });
    });

    let show_inline_reply_form =
        Signal::derive(move || active_reply_parent_id.get() == Some(parent_id));

    view! {
      <div
        class="mt-3 mb-2 border border-blue-200 rounded-lg p-3 bg-blue-50"
        style:display=move || if show_inline_reply_form.get() { "block" } else { "none" }
      >
        <ActionForm action=add_comment>
          <input type="hidden" name="form_id" value="inline" />
          <input type="hidden" name="page_id" value=page_id.to_string() />
          <input type="hidden" name="parent_id" value=parent_id.to_string() />

          <div class="flex flex-col space-y-2">
            <textarea
              class="w-full p-2 text-xs border border-gray-300 rounded outline-0"
              name="content"
              placeholder=t_string!(i18n, write_your_reply)
              prop:value=content
              on:input=move |ev| set_content.run(event_target_value(&ev))
              rows="3"
              required
            ></textarea>
            <div class="relative flex flex-wrap my-1">
              <div class="flex flex-1">
                <input
                  class="flex p-2 text-xs border border-gray-300 rounded"
                  type="text"
                  name="user_name"
                  placeholder=t_string!(i18n, user_name)
                  prop:value=user_name
                  on:input=move |ev| set_user_name.run(event_target_value(&ev))
                  required
                />
              </div>
              <div class="flex justify-end space-x-2">
                <button
                  type="button"
                  class="px-3 py-1 text-xs text-gray-600 border border-gray-300 rounded hover:bg-gray-100 cursor-pointer"
                  on:click=move |_| {
                    set_active_reply_parent_id.set(None);
                  }
                >
                  {t_string!(i18n, cancel)}
                </button>
                <button
                  type="submit"
                  class="px-3 py-1 text-xs text-white bg-blue-500 rounded hover:bg-blue-600 disabled:bg-blue-300 cursor-pointer"
                  disabled=move || add_comment.pending().get()
                >
                  {move || {
                    if add_comment.pending().get() {
                      t_string!(i18n, sending)
                    } else {
                      t_string!(i18n, submit)
                    }
                  }}

                </button>
              </div>

            </div>
          </div>
        </ActionForm>
      </div>
    }
}

fn comment_thread(
    comment: Comment,
    all_comments: HashMap<Option<i64>, Vec<Comment>>,
    active_reply_parent_id: ReadSignal<Option<i64>>,
    set_active_reply_parent_id: WriteSignal<Option<i64>>,
    add_comment: ServerAction<AddComment>,
    page_id: Arc<String>,
    reply_drafts: ReadSignal<HashMap<i64, ReplyDraft>>,
    set_reply_drafts: WriteSignal<HashMap<i64, ReplyDraft>>,
) -> AnyView {
    let i18n = use_i18n();

    let children = all_comments
        .get(&Some(comment.id))
        .cloned()
        .unwrap_or_default();

    let children_views: Vec<AnyView> = children
        .into_iter()
        .map(|child| {
            comment_thread(
                child,
                all_comments.clone(),
                active_reply_parent_id,
                set_active_reply_parent_id,
                add_comment.clone(),
                Arc::clone(&page_id),
                reply_drafts,
                set_reply_drafts,
            )
        })
        .collect();

    let comment_id = comment.id;
    let comment_user_name = comment.user_name.clone();
    let comment_content = comment.content.clone();
    let timezone_shanghai = FixedOffset::east_opt(8 * 3600).unwrap();
    let comment_date = comment
        .created_at
        .and_utc()
        .with_timezone(&timezone_shanghai)
        .to_rfc3339();

    let draft = Signal::derive(move || {
        reply_drafts.with(|d| d.get(&comment_id).cloned().unwrap_or_default())
    });
    let set_draft = Callback::new(move |new_draft: ReplyDraft| {
        set_reply_drafts.update(|d| {
            d.insert(comment_id, new_draft);
        });
    });

    // 💬 listener: toggle expanded_replay between None <--> Some(comment_id)
    let on_reply_click = move |_| {
        if active_reply_parent_id.get() == Some(comment_id) {
            set_active_reply_parent_id.set(None);
        } else {
            set_active_reply_parent_id.set(Some(comment_id));
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
            title=move || {
              if active_reply_parent_id.get() == Some(comment_id) {
                t!(i18n, cancel_reply).to_html()
              } else {
                t!(i18n, reply).to_html()
              }
            }
            on:click=on_reply_click
          >
            "💬"
          </button>
        </div>
        <p class="mt-1 text-gray-700">{comment_content.clone()}</p>

        {
          view! {
            <InlineReplyForm
              page_id=page_id.clone()
              parent_id=comment_id
              add_comment=add_comment.clone()
              active_reply_parent_id=active_reply_parent_id
              set_active_reply_parent_id=set_active_reply_parent_id
              draft=draft
              set_draft=set_draft
            />
          }
        }
        {children_views}
      </div>
    }
    .into_any()
}

#[component]
pub fn CommentSystem(page_id: String) -> impl IntoView {
    #![allow(deprecated)]
    let i18n = provide_i18n_context();

    let page_id = if page_id.is_empty() {
        let location = leptos::web_sys::window().unwrap().location();
        location.pathname().unwrap()
    } else {
        page_id
    };
    let page_id_arc = Arc::new(page_id);
    let page_id_for_children = Arc::clone(&page_id_arc);
    let page_id_for_top_form = Arc::clone(&page_id_arc);
    let page_id_for_resource = Arc::clone(&page_id_arc);

    // 当前活跃的回复表单对应的parent comment's id, 若回复成功，新增comment的parent_id
    // if the reply bubble is first clicked: current active reply's parent id, e.g, Some(comment_id)
    // if the reply bubble is clicked again: None
    let (active_reply_parent_id, set_active_reply_parent_id) = signal(None::<i64>);

    // (page_id, n_comments_submitted) -drive-> comments_resource -> comments list
    let (n_comments_submitted, set_n_comments_submitted) = signal(0 as usize);
    let add_comment = ServerAction::<AddComment>::new();
    let comments_resource = Resource::new(
        move || (n_comments_submitted.get(), page_id_for_resource.clone()),
        move |(_, pid_arc)| get_comments((*pid_arc).clone()),
    );

    let (user_name_main_form, set_user_name_main_form) = signal(String::new());
    let (content_main_form, set_content_main_form) = signal(String::new());
    // submitted_result_message is drived by inline AND main form, since submitted_result_message <- add_comment_submitted <- add_comment(action)
    // where action is resued by inline and main form
    // let (submitted_result_message, set_submitted_result_message) = signal(String::new());

    let submitted_result_message = Signal::derive(move || {
        add_comment.value().with(|d| match d {
            Some(Ok(_)) => t_string!(i18n, submitted_to_review).to_string(),
            Some(Err(_)) => t_string!(i18n, failed_try_again).to_string(),
            None => "".to_string(),
        })
    });

    // reply drafts of each comment: parent_id -> draft
    let (reply_drafts, set_reply_drafts) = signal(HashMap::<i64, ReplyDraft>::new());

    // add_comment:<Action>'s value, which is a signal, drives
    // - n_comments_submitted
    // - submitted_result_message
    // - user_name_main_form/content_main_form
    // - active_reply_parent_id
    // - reply_drafts
    Effect::new(move |_| {
        let result = add_comment.value().get();
        match result {
            Some(Ok(resp)) => {
                *set_n_comments_submitted.write() += 1;
                // log!("add_comment submitted: ok");
                // log!("  form_id={}", resp.form_id);
                match resp.form_id.as_str() {
                    "main" => {
                        // clear content of main-form
                        set_user_name_main_form.set(String::new());
                        set_content_main_form.set(String::new());
                    }
                    "inline" => {
                        // close inline-form, open main-form
                        set_active_reply_parent_id.set(None);
                        // set_show_main_form.set(true);
                        if let Some(parent_id) = resp.parent_id {
                            set_reply_drafts.update(|d| {
                                d.remove(&parent_id);
                            });
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    });

    view! {
      <I18nContextProvider>
        <div class="comment-container mt-4">
          <div class="text-sm mb-2 text-center">{submitted_result_message}</div>

          // show main-form if no active reply form (active_reply_parent_id is None)
          <div style:display=move || {
            if active_reply_parent_id.get().is_none() { "block" } else { "none" }
          }>
            <ActionForm action=add_comment>
              <input type="hidden" name="form_id" value="main" />
              <input type="hidden" name="page_id" value=page_id_for_top_form.to_string() />

              <div class="relative shrink w-full m-2 rounded-xl border border-solid border-gray-300 mb-5">
                <textarea
                  class="relative resize-y box-border w-[calc(100%-1em)] min-h-32 text-xs my-3 mx-2 rounded-xs bg-transparent outline-0"
                  name="content"
                  bind:value=(content_main_form, set_content_main_form)
                  placeholder=t_string!(i18n, welcome_comment)
                  required
                ></textarea>

                <div class="relative flex flex-wrap mx-2 my-3">
                  <div class="flex flex-1">
                    <input
                      class="resize-none w-0 text-[0.625em] flex-1 p-2 bg-transparent outline-gray-300"
                      type="text"
                      name="user_name"
                      bind:value=(user_name_main_form, set_user_name_main_form)
                      placeholder=t_string!(i18n, user_name)
                      required
                    />
                  </div>

                  <div class="flex items-center justify-end flex-3 flex-shrink">
                    <button
                      type="submit"
                      class="inline-block px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 cursor-pointer"
                    >
                      {move || {
                        if add_comment.pending().get() {
                          t_string!(i18n, sending)
                        } else {
                          t_string!(i18n, submit)
                        }
                      }}
                    </button>
                  </div>
                </div>
              </div>
            </ActionForm>
          </div>

          <div>
            <div class="text-xl">{t!(i18n, comments)}</div>
          </div>
          <Suspense fallback=|| {
            let i18n = use_i18n();
            view! { <p>{t_string!(i18n, loading)}</p> }
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
                          active_reply_parent_id,
                          set_active_reply_parent_id,
                          add_comment.clone(),
                          Arc::clone(&page_id_for_children),
                          reply_drafts,
                          set_reply_drafts,
                        )
                      })
                      .collect();
                    if root_views.is_empty() {
                      view! {
                        <p class="text-gray-400 text-center">{t!(i18n, no_comment_be_first)}</p>
                      }
                        .into_any()
                    } else {
                      view! { {root_views} }.into_any()
                    }
                  }
                  _ => {
                    view! {
                      <p class="text-gray-400 text-center">{t!(i18n, no_comment_be_first)}</p>
                    }
                      .into_any()
                  }
                })
            }}
          </Suspense>
        </div>
      </I18nContextProvider>
    }
}

/// Show review result
#[component]
fn ReviewResult() -> impl IntoView {
    use leptos_router::hooks::use_query_map;
    let query = use_query_map();
    let result = move || query.get().get("result").unwrap_or_default();

    view! {
      <div class="p-4">
        {move || match result().as_str() {
          "approved" => {
            view! {
              <h1 class="text-green-600">{format!("✅ {}", rust_i18n::t!("approved_info"))}</h1>
            }
          }
          "rejected" => {
            view! {
              <h1 class="text-red-600">{format!("✅ {}", rust_i18n::t!("rejected_info"))}</h1>
            }
          }
          "expired" => {
            view! {
              <h1 class="text-yellow-600">
                {format!("❌：⏰ {}", rust_i18n::t!("expired_info"))}
              </h1>
            }
          }
          "invalid" => {
            view! {
              <h1 class="text-yellow-600">
                {format!("❌：🚫 {}", rust_i18n::t!("invalid_info"))}
              </h1>
            }
          }
          _ => {
            view! { <h1 class="a">{format!("❌ ℹ️ {}", rust_i18n::t!("unknown_info"))}</h1> }
          }
        }}
      </div>
    }
}

/// A simple App which provides review-result route.
#[component]
pub fn App() -> impl IntoView {
    use leptos_router::path;
    view! {
      <Router>
        <Routes fallback=|| "Not found">
          <Route path=path!("/review-result") view=ReviewResult />
          <Route path=path!("/*any") view=|| view! { <h1>"Not found"</h1> } />
        </Routes>
      </Router>
    }
}
