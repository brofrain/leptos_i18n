use crate::i18n::*;
use leptos::*;

#[component]
pub fn App() -> impl IntoView {
    leptos_meta::provide_meta_context();

    let i18n = provide_i18n_context();

    let on_switch = move |_| {
        let new_lang = match i18n.get_locale() {
            LocaleEnum::en => LocaleEnum::fr,
            LocaleEnum::fr => LocaleEnum::en,
        };
        i18n.set_locale(new_lang);
    };

    view! {
        <button on:click=on_switch>{t!(i18n, click_to_change_lang)}</button>
        <Counter />
    }
}

#[component]
fn Counter() -> impl IntoView {
    let i18n = use_i18n();

    let (counter, set_counter) = create_signal(0);

    let inc = move |_| set_counter.update(|count| *count += 1);

    let count = move || counter.get();

    let b = |children: ChildrenFn| view! { <b>{children()}</b> };

    view! {
        <p>{t!(i18n, click_count, count, <b>)}</p>
        // <p>{t!(i18n, click_count, count = move || counter.get())}</p>
        <button on:click=inc>{t!(i18n, click_to_inc, <i> = |children| view! { <i>{children()}</i> })}</button>
    }
}
