use dioxus::document::eval;
use dioxus::prelude::*;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use proxynexus_core::card_store::{CardStore, clean_card_name, normalize_title};

#[derive(Props, Clone, PartialEq)]
pub struct CardListInputProps {
    pub all_cards: Resource<Option<Vec<String>>>,
    pub list_text: Signal<String>,
    pub oninput: EventHandler<String>,
}

#[component]
pub fn CardListInput(props: CardListInputProps) -> Element {
    let mut suggestions: Signal<Vec<String>> = use_signal(Vec::new);
    let mut pending_cursor_pos = use_signal(|| None::<usize>);
    let mut is_suggestions_visible = use_signal(|| false);
    let mut dropdown_visual_line_idx = use_signal(|| 0usize);
    let mut highlighted_suggestion_idx = use_signal(|| 0usize);
    let mut cursor_line_idx = use_signal(|| 0usize);

    use_effect(move || {
        if let Some(pos) = pending_cursor_pos() {
            let _ = eval(&format!(
                "
                let el = document.getElementById('card-list-input');
                if (el) {{
                    el.focus();
                    el.selectionStart = {};
                    el.selectionEnd = {};
                }}
                ",
                pos, pos
            ));
            pending_cursor_pos.set(None);
        }
    });

    let handle_input = move |evt: Event<FormData>| {
        let text = evt.value();
        props.oninput.call(text.clone());

        let all_cards = match props.all_cards.read().as_ref() {
            Some(Some(cards)) => cards.clone(),
            _ => return,
        };

        spawn(async move {
            let mut eval = eval(
                "
                let el = document.getElementById('card-list-input');
                dioxus.send(el ? el.selectionStart : 0);
                ",
            );

            let cursor_pos = if let Ok(val) = eval.recv::<usize>().await {
                val
            } else {
                text.chars().count()
            };

            let line_idx = text.chars().take(cursor_pos).filter(|&c| c == '\n').count();
            let lines: Vec<&str> = text.split('\n').collect();

            if lines.is_empty() || line_idx >= lines.len() {
                is_suggestions_visible.set(false);
                return;
            }

            cursor_line_idx.set(line_idx);
            let current_line = lines[line_idx];

            let visual_lines_before: usize = lines
                .iter()
                .take(line_idx)
                .map(|line| (line.chars().count() / 44) + 1)
                .sum();
            let visual_lines_current = current_line.chars().count() / 44;
            dropdown_visual_line_idx.set(visual_lines_before + visual_lines_current);

            let (_qty, rest) = CardStore::parse_quantity(current_line);
            let name = match CardStore::parse_overrides(rest) {
                Ok((n, _, _, _)) => clean_card_name(n),
                Err(_) => clean_card_name(rest),
            }
            .trim();

            if name.len() < 3 {
                is_suggestions_visible.set(false);
                return;
            }

            let matcher = SkimMatcherV2::default();
            let normalized_name = normalize_title(name);

            let mut matches: Vec<(i64, String)> = all_cards
                .iter()
                .filter_map(|card| {
                    let score_original = matcher.fuzzy_match(card, name).unwrap_or(0);
                    let score_normalized = matcher
                        .fuzzy_match(&normalize_title(card), &normalized_name)
                        .unwrap_or(0);
                    let score = std::cmp::max(score_original, score_normalized);

                    if score > 0 {
                        Some((score, card.clone()))
                    } else {
                        None
                    }
                })
                .collect();

            matches.sort_by_key(|b| std::cmp::Reverse(b.0));
            let top_matches: Vec<String> = matches.into_iter().take(5).map(|(_, s)| s).collect();

            if top_matches.is_empty() {
                is_suggestions_visible.set(false);
            } else {
                suggestions.set(top_matches);
                highlighted_suggestion_idx.set(0);
                is_suggestions_visible.set(true);
            }
        });
    };

    let oninput = props.oninput;
    let list_text = props.list_text;

    let apply_suggestion = move |selected_card: String, insert_newline: bool| {
        let mut is_suggestions_visible = is_suggestions_visible;
        let mut pending_cursor_pos = pending_cursor_pos;
        let text = list_text.read().clone();
        let lines: Vec<&str> = text.split('\n').collect();

        if let Some(line) = lines.get(cursor_line_idx()) {
            let (qty, rest) = CardStore::parse_quantity(line);

            let overrides_part = if let Some(bracket_start) = rest.find('[') {
                &rest[bracket_start..]
            } else {
                ""
            };

            let mut new_line = String::new();
            if qty > 1 {
                new_line.push_str(&format!("{}x {}", qty, selected_card));
            } else {
                new_line.push_str(&selected_card);
            }

            if !overrides_part.is_empty() {
                new_line.push(' ');
                new_line.push_str(overrides_part);
            }

            let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
            if new_lines.is_empty() {
                new_lines.push(new_line);
            } else {
                new_lines[cursor_line_idx()] = new_line;
            }

            if insert_newline {
                new_lines.insert(cursor_line_idx() + 1, String::new());
            }

            let new_text = new_lines.join("\n");
            oninput.call(new_text.clone());
            is_suggestions_visible.set(false);

            let _ = eval(&format!(
                "
                let el = document.getElementById('card-list-input');
                if (el) {{ el.value = `{}`; }}
                ",
                new_text.replace('`', "\\`").replace('$', "\\$")
            ));

            let target_line = if insert_newline {
                cursor_line_idx() + 1
            } else {
                cursor_line_idx()
            };

            let mut pos: usize = new_lines
                .iter()
                .take(target_line)
                .map(|l| l.chars().count())
                .sum::<usize>()
                + target_line;

            if !insert_newline {
                pos += new_lines[target_line].chars().count();
            }

            pending_cursor_pos.set(Some(pos));
        }
    };

    let handle_keydown = move |evt: Event<KeyboardData>| {
        if !is_suggestions_visible() {
            return;
        }

        match evt.key() {
            Key::ArrowDown => {
                evt.prevent_default();
                let current = highlighted_suggestion_idx();
                let max = suggestions.read().len().saturating_sub(1);
                if current < max {
                    highlighted_suggestion_idx.set(current + 1);
                }
            }
            Key::ArrowUp => {
                evt.prevent_default();
                let current = highlighted_suggestion_idx();
                if current > 0 {
                    highlighted_suggestion_idx.set(current - 1);
                }
            }
            Key::Enter | Key::Tab => {
                evt.prevent_default();
                let insert_newline = evt.key() == Key::Enter;
                let selected = {
                    let sugs = suggestions.read();
                    if let Some(s) = sugs.get(highlighted_suggestion_idx()) {
                        s.clone()
                    } else {
                        return;
                    }
                };
                apply_suggestion(selected, insert_newline);
            }
            Key::Escape => {
                evt.prevent_default();
                is_suggestions_visible.set(false);
            }
            _ => {}
        }
    };

    rsx! {
        div {
            class: "relative flex-1 w-full flex flex-col",
            textarea {
                id: "card-list-input",
                class: "flex-1 w-full p-3 border border-gray-300 rounded-md shadow-sm outline-none focus:ring-2 focus:ring-blue-400 resize-none font-mono text-base md:text-sm",
                placeholder: "Enter your card list here (e.g. 3x Sure Gamble)...",
                initial_value: "{props.list_text.peek()}",
                oninput: handle_input,
                onkeydown: handle_keydown,
            }

            if is_suggestions_visible() {
                div {
                    class: "absolute z-10 bg-white border border-gray-300 rounded-md shadow-lg",
                    style: "top: {36.0 + (dropdown_visual_line_idx() as f64 * 20.0)}px; left: 16px; min-width: 200px;",

                    for (i, suggestion) in suggestions.read().iter().enumerate() {
                        div {
                            class: if i == highlighted_suggestion_idx() {
                                "px-4 py-2 cursor-pointer bg-blue-100 text-blue-900 text-base md:text-sm font-mono"
                            } else {
                                "px-4 py-2 cursor-pointer hover:bg-gray-100 text-base md:text-sm font-mono"
                            },
                            onclick: {
                                let sug = suggestion.clone();
                                move |_| apply_suggestion(sug.clone(), false)
                            },
                            "{suggestion}"
                        }
                    }
                }
            }
        }
    }
}
