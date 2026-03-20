use dioxus::prelude::*;
use proxynexus_core::card_store::CardStore;
use proxynexus_core::db_storage::DbStorage;

#[derive(Clone, PartialEq, Debug)]
pub enum ActiveSource {
    Cardlist(String),
    SetName(String),
    NrdbUrl(String),
}

impl Default for ActiveSource {
    fn default() -> Self {
        ActiveSource::Cardlist(String::new())
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct SourceSelectorProps {
    pub source_state: Signal<ActiveSource>,
    pub db_signal: Signal<DbStorage>,
    pub on_source_changed: EventHandler<()>,
}

#[component]
pub fn SourceSelector(props: SourceSelectorProps) -> Element {
    let mut tab = use_signal(|| "list");
    let mut db_signal = props.db_signal;
    let mut source_state = props.source_state;

    let mut list_text = use_signal(String::new);
    let mut set_name = use_signal(String::new);
    let mut nrdb_url = use_signal(String::new);

    let available_sets = use_resource(move || async move {
        let mut db = db_signal.write();
        match CardStore::new(&mut db) {
            Ok(mut store) => {
                let packs = store.get_available_packs().await.unwrap_or_default();
                packs
                    .into_iter()
                    .filter(|(_, meta)| !meta.contains("no printings available"))
                    .collect::<Vec<_>>()
            }
            Err(_) => Vec::new(),
        }
    });

    rsx! {
        div {
            class: "flex flex-col flex-1 p-4 w-full",
            h2 { class: "text-lg font-bold mb-4 text-gray-800", "Source" }

            div { class: "flex border-b border-gray-200 mb-4",
                button {
                    class: if tab() == "list" { "px-4 py-2 border-b-2 border-blue-600 text-blue-600 text-sm font-semibold -mb-[1px]" } else { "px-4 py-2 text-gray-500 text-sm font-medium hover:text-gray-700 border-b-2 border-transparent -mb-[1px]" },
                    onclick: move |_| {
                        if tab() != "list" {
                            props.on_source_changed.call(());
                        }
                        tab.set("list");
                        source_state.set(ActiveSource::Cardlist(list_text()));
                    },
                    "List"
                }
                button {
                    class: if tab() == "set" { "px-4 py-2 border-b-2 border-blue-600 text-blue-600 text-sm font-semibold -mb-[1px]" } else { "px-4 py-2 text-gray-500 text-sm font-medium hover:text-gray-700 border-b-2 border-transparent -mb-[1px]" },
                    onclick: move |_| {
                        if tab() != "set" {
                            props.on_source_changed.call(());
                        }
                        tab.set("set");
                        source_state.set(ActiveSource::SetName(set_name()));
                    },
                    "Set"
                }
                button {
                    class: if tab() == "nrdb" { "px-4 py-2 border-b-2 border-blue-600 text-blue-600 text-sm font-semibold -mb-[1px]" } else { "px-4 py-2 text-gray-500 text-sm font-medium hover:text-gray-700 border-b-2 border-transparent -mb-[1px]" },
                    onclick: move |_| {
                        if tab() != "nrdb" {
                            props.on_source_changed.call(());
                        }
                        tab.set("nrdb");
                        source_state.set(ActiveSource::NrdbUrl(nrdb_url()));
                    },
                    "NetrunnerDB"
                }
            }

            match tab() {
                "list" => rsx! {
                    textarea {
                        class: "flex-1 w-full p-3 border border-gray-300 rounded-md shadow-sm outline-none focus:ring-2 focus:ring-blue-400 resize-none font-mono text-sm",
                        placeholder: "Enter your card list here (e.g. 3x Sure Gamble)...",
                        value: "{list_text}",
                        oninput: move |evt| {
                            list_text.set(evt.value());
                            source_state.set(ActiveSource::Cardlist(evt.value()));
                        }
                    }
                },
                "set" => rsx! {
                    select {
                        class: "w-full p-2 border border-gray-300 rounded-md outline-none focus:ring-2 focus:ring-blue-400 bg-white text-sm",
                        value: "{set_name}",
                        onchange: move |evt| {
                            props.on_source_changed.call(());
                            set_name.set(evt.value());
                            source_state.set(ActiveSource::SetName(evt.value()));
                        },
                        option { value: "", disabled: true, "Select a set..." }
                        if let Some(sets) = available_sets.read().as_ref() {
                            for (name, _code) in sets.iter().rev() {
                                option { value: "{name}", "{name}" }
                            }
                        }
                    }
                },
                "nrdb" => rsx! {
                    input {
                        type: "text",
                        class: "w-full p-3 border border-gray-300 rounded-md shadow-sm outline-none focus:ring-2 focus:ring-blue-400 font-mono text-sm",
                        placeholder: "https://netrunnerdb.com/en/decklist/...",
                        value: "{nrdb_url}",
                        oninput: move |evt| {
                            props.on_source_changed.call(());
                            nrdb_url.set(evt.value());
                            source_state.set(ActiveSource::NrdbUrl(evt.value()));
                        }
                    }
                },
                _ => rsx! { div {} }
            }
        }
    }
}
