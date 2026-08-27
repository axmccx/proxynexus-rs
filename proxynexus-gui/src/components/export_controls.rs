use crate::export::ExportOptions;
use dioxus::prelude::*;
use proxynexus_core::card_backs;
use proxynexus_core::mpc::MpcOptions;
use proxynexus_core::pdf::{
    CutLines, DEFAULT_CUT_LINE_THICKNESS, MAX_CUT_LINE_THICKNESS, MIN_CUT_LINE_THICKNESS, PageSize,
    PdfOptions, PrintLayout,
};

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ExportFormat {
    Pdf,
    Mpc,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PageSizePreset {
    Letter,
    A4,
    Custom,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CustomUnit {
    In,
    Cm,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Sides {
    #[default]
    Single,
    Double,
}

#[derive(Props, Clone, PartialEq, Debug)]
struct SegmentedControlProps<T: PartialEq + Copy + 'static> {
    value: T,
    options: Vec<(T, &'static str)>,
    on_change: EventHandler<T>,
    disabled: bool,
}

#[component]
fn SegmentedControl<T: PartialEq + Copy + 'static>(props: SegmentedControlProps<T>) -> Element {
    rsx! {
        div {
            class: "flex flex-wrap p-1 bg-gray-200 rounded-lg gap-1",
            role: "radiogroup",
            for (opt_value, label) in props.options {
                {
                    let is_active = props.value == opt_value;
                    let base_class = "flex-1 text-center py-1 md:py-1.5 px-2 md:px-3 font-medium rounded-md text-xs md:text-sm \
                                      transition-all focus:outline-none focus-visible:ring-2 \
                                      focus-visible:ring-blue-400 whitespace-nowrap";
                    let state_class = if is_active {
                        "bg-white text-blue-600 shadow-sm"
                    } else {
                        "text-gray-600 hover:text-gray-900 hover:bg-gray-300/50"
                    };

                    rsx! {
                        button {
                            disabled: props.disabled,
                            role: "radio",
                            aria_checked: is_active,
                            class: "{base_class} {state_class}",
                            onclick: move |_| props.on_change.call(opt_value),
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ExportControlsProps {
    pub progress: Signal<Option<f32>>,
    pub is_disabled: bool,
    pub on_generate: EventHandler<ExportOptions>,
    pub on_open_info: EventHandler<(f64, f64, f64)>,
    pub on_open_upscale_info: EventHandler<(f64, f64, f64)>,
    pub on_open_sides_info: EventHandler<(f64, f64, f64)>,
    pub active_game_id: Signal<Option<String>>,
}

#[derive(Clone, PartialEq, Debug)]
struct PageSizeValidation {
    result: Option<PageSize>,
    width_invalid: bool,
    height_invalid: bool,
}

#[component]
pub fn ExportControls(props: ExportControlsProps) -> Element {
    let mut export_format = use_signal(|| ExportFormat::Pdf);
    let mut page_size_preset = use_signal(|| PageSizePreset::Letter);
    let mut cut_lines = use_signal(CutLines::default);
    let mut cut_line_thickness = use_signal(|| DEFAULT_CUT_LINE_THICKNESS.to_string());
    let mut print_layout = use_signal(PrintLayout::default);
    let mut sides = use_signal(Sides::default);
    let mut back_label = use_signal(|| None::<&'static str>);

    let active_game_id = props.active_game_id;
    let back_labels =
        use_memo(move || active_game_id().map_or_else(Vec::new, |id| card_backs::back_labels(&id)));

    let default_back_label =
        use_memo(move || active_game_id().and_then(|id| card_backs::default_label(&id)));

    use_effect(move || {
        if back_label().is_some_and(|label| !back_labels().contains(&label)) {
            back_label.set(None);
        }
    });
    let mut upscale = use_signal(|| false);
    let mut show_donate = use_signal(|| false);

    let thickness_value = use_memo(move || {
        cut_line_thickness()
            .parse::<f32>()
            .ok()
            .filter(|v| (MIN_CUT_LINE_THICKNESS..=MAX_CUT_LINE_THICKNESS).contains(v))
    });

    let mut custom_width = use_signal(|| "".to_string());
    let mut custom_height = use_signal(|| "".to_string());
    let mut custom_unit = use_signal(|| CustomUnit::In);

    let is_generating = (props.progress)().is_some();
    let is_gpu_available = *crate::GPU_AVAILABLE.read();

    let page_size_validation = use_memo(move || -> PageSizeValidation {
        match page_size_preset() {
            PageSizePreset::A4 => PageSizeValidation {
                result: Some(PageSize::A4),
                width_invalid: false,
                height_invalid: false,
            },
            PageSizePreset::Custom => {
                let w_text = custom_width();
                let h_text = custom_height();

                let w = w_text.parse::<f32>();
                let h = h_text.parse::<f32>();

                let factor = if custom_unit() == CustomUnit::Cm {
                    1.0 / 2.54
                } else {
                    1.0
                };

                let w_valid = matches!(w, Ok(v) if (v * factor) > 0.0 && (v * factor) <= 60.0);
                let h_valid = matches!(h, Ok(v) if (v * factor) > 0.0 && (v * factor) <= 60.0);

                let result = match (w, h) {
                    (Ok(w_val), Ok(h_val)) if w_valid && h_valid => {
                        Some(PageSize::Custom(w_val * factor, h_val * factor))
                    }
                    _ => None,
                };

                PageSizeValidation {
                    result,
                    width_invalid: !w_valid && !w_text.is_empty(),
                    height_invalid: !h_valid && !h_text.is_empty(),
                }
            }
            PageSizePreset::Letter => PageSizeValidation {
                result: Some(PageSize::Letter),
                width_invalid: false,
                height_invalid: false,
            },
        }
    });

    let validation = page_size_validation();

    let instructions_href = if cfg!(target_arch = "wasm32") {
        "/instructions.html"
    } else {
        "https://proxynexus.net/instructions.html"
    };

    rsx! {
        div {
            class: "md:h-[480px] flex-shrink-0 p-2 md:p-4 border-t border-gray-200 bg-gray-50 flex flex-col gap-2 md:gap-4 overflow-y-auto",

            div { class: "flex flex-col gap-1 md:gap-2",
                label { class: "text-xs md:text-sm font-medium text-gray-700", "Format" }
                SegmentedControl {
                    value: export_format(),
                    disabled: is_generating,
                    options: vec![
                        (ExportFormat::Pdf, "PDF"),
                        (ExportFormat::Mpc, "MPC"),
                    ],
                    on_change: move |v| export_format.set(v)
                }
            }

            if export_format() == ExportFormat::Pdf {
                div { class: "flex flex-col gap-1 md:gap-2",
                    label { class: "text-xs md:text-sm font-medium text-gray-700", "Page Size" }
                    SegmentedControl {
                        value: page_size_preset(),
                        disabled: is_generating,
                        options: vec![
                            (PageSizePreset::Letter, "Letter"),
                            (PageSizePreset::A4, "A4"),
                            (PageSizePreset::Custom, "Custom"),
                        ],
                        on_change: move |v| page_size_preset.set(v)
                    }
                }

                if page_size_preset() == PageSizePreset::Custom {
                    div { class: "flex gap-2 items-start pt-2",
                        {
                            let base = "w-full p-2 border rounded-md outline-none focus:ring-2 text-xs md:text-sm transition-all";
                            let w_state = if validation.width_invalid {
                                "border-red-500 focus:ring-red-400 bg-red-50"
                            } else {
                                "border-gray-300 focus:ring-blue-400 bg-white"
                            };
                            let h_state = if validation.height_invalid {
                                "border-red-500 focus:ring-red-400 bg-red-50"
                            } else {
                                "border-gray-300 focus:ring-blue-400 bg-white"
                            };

                            rsx! {
                                div { class: "flex flex-col w-full gap-1",
                                    input {
                                        disabled: is_generating,
                                        class: "{base} {w_state}",
                                        type: "text",
                                        placeholder: "Width",
                                        value: "{custom_width()}",
                                        oninput: move |evt| custom_width.set(evt.value().clone())
                                    }
                                    if validation.width_invalid {
                                        span { class: "text-xs text-red-500 font-medium", "Invalid" }
                                    }
                                }
                                div { class: "flex flex-col w-full gap-1",
                                    input {
                                        disabled: is_generating,
                                        class: "{base} {h_state}",
                                        type: "text",
                                        placeholder: "Height",
                                        value: "{custom_height()}",
                                        oninput: move |evt| custom_height.set(evt.value().clone())
                                    }
                                    if validation.height_invalid {
                                        span { class: "text-xs text-red-500 font-medium", "Invalid" }
                                    }
                                }
                                select {
                                    disabled: is_generating,
                                    class: "p-2 border border-gray-300 rounded-md outline-none focus:ring-2 focus:ring-blue-400 bg-white text-xs md:text-sm h-[38px] shrink-0",
                                    value: match custom_unit() {
                                        CustomUnit::In => "in",
                                        CustomUnit::Cm => "cm",
                                    },
                                    onchange: move |evt| {
                                        let val = match evt.value().as_str() {
                                            "cm" => CustomUnit::Cm,
                                            _ => CustomUnit::In,
                                        };
                                        custom_unit.set(val);
                                    },
                                    option { value: "in", "in" }
                                    option { value: "cm", "cm" }
                                }
                            }
                        }
                    }
                }

                div { class: "flex flex-col gap-1 md:gap-2",
                    label { class: "text-xs md:text-sm font-medium text-gray-700", "Cut Lines" }
                    SegmentedControl {
                        value: cut_lines(),
                        disabled: is_generating,
                        options: vec![
                            (CutLines::None, "None"),
                            (CutLines::Margins, "Margins"),
                            (CutLines::FullPage, "Full Page"),
                        ],
                        on_change: move |v| cut_lines.set(v)
                    }

                    if cut_lines() == CutLines::FullPage {
                        {
                            let is_invalid = thickness_value().is_none();
                            let base = "w-full p-2 border rounded-md outline-none focus:ring-2 text-xs md:text-sm transition-all";
                            let state = if is_invalid {
                                "border-red-500 focus:ring-red-400 bg-red-50"
                            } else {
                                "border-gray-300 focus:ring-blue-400 bg-white"
                            };
                            rsx! {
                                div { class: "flex items-center gap-2 pt-1",
                                    label { class: "text-xs md:text-sm text-gray-600 shrink-0", "Thickness (pt)" }
                                    input {
                                        disabled: is_generating,
                                        class: "{base} {state}",
                                        type: "text",
                                        value: "{cut_line_thickness()}",
                                        oninput: move |evt| cut_line_thickness.set(evt.value().clone())
                                    }
                                }
                                if is_invalid {
                                    span { class: "text-xs text-red-500 font-medium",
                                        "Enter a number between {MIN_CUT_LINE_THICKNESS} and {MAX_CUT_LINE_THICKNESS}"
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "flex flex-col gap-1 md:gap-2",
                    div { class: "flex items-center gap-2",
                        label { class: "text-xs md:text-sm font-medium text-gray-700", "Print Layout" }
                        button {
                            id: "print-layout-info-btn",
                            class: "text-gray-400 hover:text-blue-500 transition-colors focus:outline-none",
                            onclick: move |_| {
                                spawn(async move {
                                    let mut eval = dioxus::document::eval(
                                        "
                                        let el = document.getElementById('print-layout-info-btn');
                                        let rect = el.getBoundingClientRect();
                                        dioxus.send([rect.x, rect.y, rect.width]);
                                        ",
                                    );
                                    if let Ok((x, y, w)) = eval.recv::<(f64, f64, f64)>().await {
                                        props.on_open_info.call((x, y, w));
                                    }
                                });
                            },
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "18",
                                height: "18",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "10" }
                                path { d: "M12 16v-4" }
                                path { d: "M12 8h.01" }
                            }
                        }
                    }
                    SegmentedControl {
                        value: print_layout(),
                        disabled: is_generating,
                        options: vec![
                            (PrintLayout::EdgeToEdge, "Edge"),
                            (PrintLayout::Gap, "Gap"),
                            (PrintLayout::Margin, "Margin"),
                            (PrintLayout::Bleed, "Bleed"),
                        ],
                        on_change: move |v| print_layout.set(v)
                    }
                }

                div { class: "flex flex-col gap-1 md:gap-2",
                    div { class: "flex items-center gap-2",
                        label { class: "text-xs md:text-sm font-medium text-gray-700", "Sides" }
                        button {
                            id: "sides-info-btn",
                            class: "text-gray-400 hover:text-blue-500 transition-colors focus:outline-none",
                            onclick: move |_| {
                                spawn(async move {
                                    let mut eval = dioxus::document::eval(
                                        "
                                        let el = document.getElementById('sides-info-btn');
                                        let rect = el.getBoundingClientRect();
                                        dioxus.send([rect.x, rect.y, rect.width]);
                                        ",
                                    );
                                    if let Ok((x, y, w)) = eval.recv::<(f64, f64, f64)>().await {
                                        props.on_open_sides_info.call((x, y, w));
                                    }
                                });
                            },
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "18",
                                height: "18",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "10" }
                                path { d: "M12 16v-4" }
                                path { d: "M12 8h.01" }
                            }
                        }
                    }
                    SegmentedControl {
                        value: sides(),
                        disabled: is_generating,
                        options: vec![(Sides::Single, "Single"), (Sides::Double, "Double")],
                        on_change: move |v| sides.set(v)
                    }

                    if sides() == Sides::Double && back_labels().len() > 1 {
                        div { class: "flex items-center gap-2",
                            label { class: "text-xs md:text-sm text-gray-600 shrink-0", "Back art" }
                            select {
                                disabled: is_generating,
                                class: "w-full p-2 border border-gray-300 rounded-md bg-white outline-none focus:ring-2 focus:ring-blue-400 text-xs md:text-sm",
                                value: back_label().or(default_back_label()).unwrap_or_default(),
                                onchange: move |evt| {
                                    let chosen = evt.value();
                                    back_label
                                        .set(back_labels().into_iter().find(|label| *label == chosen));
                                },
                                for label in back_labels() {
                                    option { value: "{label}", "{display_label(label)}" }
                                }
                            }
                        }
                    }
                }
            } else {
                p { class: "text-xs md:text-sm text-gray-600",
                    "For help on printing, have a look at the "
                    a {
                        href: instructions_href,
                        target: "_blank",
                        class: "text-blue-500 hover:text-blue-700 hover:underline",
                        "instructions"
                    }
                }
            }

            div { class: "mt-auto pt-4 md:pt-0 flex flex-col gap-2",
                if show_donate() {
                    div { class: "text-sm text-center text-gray-600 pb-1",
                        "Finding this site useful? Please consider "
                        a {
                            href: "https://ko-fi.com/axmccx",
                            target: "_blank",
                            class: "text-blue-500 hover:text-blue-700 hover:underline",
                            "donating"
                        }
                    }
                    div { class: "text-sm text-center text-gray-600 pb-1",
                        "Issues? Report it "
                        a {
                            href: "https://github.com/axmccx/proxynexus-rs/issues/new",
                            target: "_blank",
                            class: "text-blue-500 hover:text-blue-700 hover:underline",
                            "here"
                        }
                    }
                }

                if let Some(p) = (props.progress)() {
                    div { class: "flex flex-col gap-2",
                        div { class: "w-full bg-gray-200 rounded-full h-4 overflow-hidden",
                            div {
                                class: "bg-blue-600 h-full transition-all duration-75",
                                style: "width: {p * 100.0}%",
                            }
                        }
                        div { class: "text-xs text-center text-gray-500 font-medium",
                            "{ (p * 100.0) as u32 }%"
                        }
                    }
                } else {
                    div {
                        class: "flex items-center gap-3 md:gap-4",

                        div { class: "flex flex-col items-center gap-1 shrink-0",
                            div {
                                class: "flex items-center gap-1 cursor-help group",
                                onclick: move |_| {
                                    spawn(async move {
                                        let mut eval = dioxus::document::eval(
                                            "
                                            let el = document.getElementById('upscale-info-btn');
                                            let rect = el.getBoundingClientRect();
                                            dioxus.send([rect.x, rect.y, rect.width]);
                                            ",
                                        );
                                        if let Ok((x, y, w)) = eval.recv::<(f64, f64, f64)>().await {
                                            props.on_open_upscale_info.call((x, y, w));
                                        }
                                    });
                                },
                                label { class: "text-xs md:text-sm font-medium text-gray-700 group-hover:text-blue-600 transition-colors cursor-help", "Upscale" }
                                div {
                                    id: "upscale-info-btn",
                                    class: "text-gray-400 group-hover:text-blue-500 transition-colors",
                                    svg {
                                        xmlns: "http://www.w3.org/2000/svg",
                                        width: "12",
                                        height: "12",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2.5",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        circle { cx: "12", cy: "12", r: "10" }
                                        path { d: "M12 16v-4" }
                                        path { d: "M12 8h.01" }
                                    }
                                }
                            }

                            button {
                                class: format!("relative inline-flex h-4 w-8 items-center rounded-full transition-colors focus:outline-none {}",
                                    if upscale() { "bg-blue-600" } else { "bg-gray-300" }),
                                disabled: !is_gpu_available,
                                onclick: move |_| upscale.set(!upscale()),
                                span {
                                    class: format!("inline-block h-3 w-3 transform rounded-full bg-white transition-transform shadow-sm {}",
                                        if upscale() { "translate-x-4.5" } else { "translate-x-0.5" }),
                                }
                            }
                        }

                        div {
                            class: "flex-1",
                            {
                                let thickness_invalid = cut_lines() == CutLines::FullPage && thickness_value().is_none();
                                let disabled = props.is_disabled || thickness_invalid;
                                let btn_base = "w-full py-1.5 md:py-2 px-3 md:px-4 font-semibold rounded-md shadow-sm transition-colors focus:outline-none focus:ring-2 focus:ring-blue-400 focus:ring-offset-1 text-xs md:text-sm";
                                let btn_state = if disabled {
                                    "bg-gray-300 text-gray-500 cursor-not-allowed"
                                } else {
                                    "bg-blue-600 hover:bg-blue-700 text-white"
                                };

                                rsx! {
                                    button {
                                        class: "{btn_base} {btn_state}",
                                        disabled,
                                        onclick: move |_| {
                                            if disabled { return; }
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                let generated = has_generated_before();
                                                if generated {
                                                    show_donate.set(true);
                                                } else {
                                                    set_has_generated();
                                                }
                                            }
                                            #[cfg(not(target_arch = "wasm32"))]
                                            {
                                                show_donate.set(true);
                                            }
                                            let options = match export_format() {
                                                ExportFormat::Mpc => {
                                                    ExportOptions::Mpc(MpcOptions { upscale: upscale() })
                                                }
                                                ExportFormat::Pdf => {
                                                    let Some(page_size) = validation.result else { return };
                                                    let thickness = match cut_lines() {
                                                        CutLines::None => 0.0,
                                                        CutLines::Margins => DEFAULT_CUT_LINE_THICKNESS,
                                                        CutLines::FullPage => match thickness_value() {
                                                            Some(t) => t,
                                                            None => return,
                                                        },
                                                    };
                                                    ExportOptions::Pdf(PdfOptions {
                                                        page_size,
                                                        cut_lines: cut_lines(),
                                                        print_layout: print_layout(),
                                                        cut_line_thickness: thickness,
                                                        upscale: upscale(),
                                                        double_sided: sides() == Sides::Double,
                                                        back_label: back_label(),
                                                    })
                                                }
                                            };
                                            props.on_generate.call(options);
                                        },
                                        "Generate"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn has_generated_before() -> bool {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(local_storage)) = window.local_storage() {
            return local_storage
                .get_item("proxynexus_has_generated")
                .unwrap_or(None)
                .is_some();
        }
    }
    false
}

#[cfg(target_arch = "wasm32")]
fn set_has_generated() {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(local_storage)) = window.local_storage() {
            let _ = local_storage.set_item("proxynexus_has_generated", "true");
        }
    }
}

fn display_label(label: &str) -> String {
    label
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
