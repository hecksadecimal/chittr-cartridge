//! The one seam between the SDK and the host. Every host function is reached through a function of the same
//! name here, taking the request JSON and returning the reply JSON, as the host's ABI does. On wasm these call
//! the imports. With the `testing` feature on a native target they answer from the in-memory fake in
//! `crate::testing` instead, so Hook logic runs under `cargo test`.

macro_rules! host_functions {
    ($($name:ident),* $(,)?) => {
        #[cfg(not(all(feature = "testing", not(target_family = "wasm"))))]
        mod imports {
            use extism_pdk::host_fn;

            #[host_fn]
            extern "ExtismHost" {
                $(pub fn $name(request: String) -> String;)*
            }
        }

        $(
            #[cfg(not(all(feature = "testing", not(target_family = "wasm"))))]
            pub(crate) unsafe fn $name(request: String) -> Result<String, extism_pdk::Error> {
                imports::$name(request)
            }

            #[cfg(all(feature = "testing", not(target_family = "wasm")))]
            pub(crate) unsafe fn $name(request: String) -> Result<String, extism_pdk::Error> {
                Ok(crate::testing::host_call(stringify!($name), &request))
            }
        )*
    };
}

host_functions! {
    // KV (kv.rs)
    kv_get,
    kv_set,
    kv_set_if_absent,
    kv_compare_and_swap,
    kv_delete,
    kv_incr,
    kv_list,
    kv_mget,
    kv_get_all,
    // Timers (timer.rs)
    schedule_timer,
    cancel_timer,
    // Render Pushes (push.rs)
    push_render,
    // Assets (host.rs)
    asset,
    // Files (host.rs, ABI v2)
    file_read,
    file_close,
    // Poster Actions (poster.rs)
    post_reply,
    post_chitt,
    edit_chitt,
    delete_chitt,
    react,
    unreact,
    rechitt,
    unrechitt,
    follow,
    unfollow,
    pin,
    unpin,
    admin_delete_post,
    admin_edit_post,
    admin_pin_chitt,
    admin_unpin_chitt,
    ban_user,
    unban_user,
    suspend_character,
    unsuspend_character,
    admin_delete_character,
    admin_update_trolltag,
    admin_update_character_name,
    admin_update_character_bio,
    admin_update_character_discriminator,
    create_badge,
    delete_badge,
    grant_badge_to_user,
    revoke_badge_from_user,
    grant_badge_to_character,
    revoke_badge_from_character,
}
