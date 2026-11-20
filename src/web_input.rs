use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{Event, HtmlInputElement};

thread_local! {
    static INPUT: RefCell<Option<WebInput>> = const { RefCell::new(None) };
    static EVENTS: RefCell<Vec<egui::Event>> = const { RefCell::new(Vec::new()) };
}

struct WebInput {
    input: HtmlInputElement,
    // Keep the closures alive for as long as their DOM event listeners are registered.
    _listeners: Vec<Closure<dyn FnMut(Event)>>,
    focused: Rc<Cell<bool>>,
    ime_enabled: Rc<Cell<bool>>,
    composing: Rc<Cell<bool>>,
    suppress_composition_end: Rc<Cell<bool>>,
    preedit_text: Rc<RefCell<String>>,
    last_text: Rc<RefCell<String>>,
}

pub(crate) fn install() {
    INPUT.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(element) = document.create_element("input") else {
            return;
        };
        let Ok(input) = element.dyn_into::<HtmlInputElement>() else {
            return;
        };
        let _ = input.set_attribute("type", "text");
        let _ = input.set_attribute("autocapitalize", "off");
        let _ = input.set_attribute("autocomplete", "off");
        let _ = input.set_attribute("aria-hidden", "true");
        let style = input.style();

        for (name, value) in [
            ("position", "fixed"),
            ("left", "0"),
            ("top", "0"),
            ("width", "1px"),
            ("height", "1px"),
            ("padding", "0"),
            ("border", "0"),
            ("opacity", "0"),
            ("pointer-events", "none"),
            ("font-size", "16px"),
        ] {
            let _ = style.set_property(name, value);
        }

        if let Some(body) = document.body() {
            let _ = body.append_child(&input);
        }

        let composing = Rc::new(Cell::new(false));
        let suppress_composition_end = Rc::new(Cell::new(false));
        let preedit_text = Rc::new(RefCell::new(String::new()));
        let last_text = Rc::new(RefCell::new(String::new()));
        let focused = Rc::new(Cell::new(false));
        let ime_enabled = Rc::new(Cell::new(false));
        let mut listeners = Vec::new();

        {
            let composing = composing.clone();
            let suppress_composition_end = suppress_composition_end.clone();
            let preedit_text = preedit_text.clone();
            add_listener(&input, "compositionstart", &mut listeners, move |_| {
                composing.set(true);
                suppress_composition_end.set(false);
                preedit_text.borrow_mut().clear();
            });
        }

        {
            let composing = composing.clone();
            let suppress_composition_end = suppress_composition_end.clone();
            let preedit_text = preedit_text.clone();
            let last_text = last_text.clone();
            let input_for_event = input.clone();
            add_listener(&input, "compositionend", &mut listeners, move |event| {
                composing.set(false);
                if suppress_composition_end.replace(false) {
                    preedit_text.borrow_mut().clear();
                    // Keep the committed value until the browser's trailing input event can be deduplicated.
                    return;
                }
                let Ok(event) = event.dyn_into::<web_sys::CompositionEvent>() else {
                    return;
                };
                let mut text = input_for_event.value();
                let prefix_len = last_text.borrow().chars().count();
                let mut commit_text = text.chars().skip(prefix_len).collect::<String>();
                if text.is_empty() && commit_text.is_empty() {
                    commit_text = event.data().unwrap_or_default();
                    text = commit_text.clone();
                    input_for_event.set_value(&text);
                }
                preedit_text.borrow_mut().clear();
                // Some IMEs emit a final input event after compositionend; retain the value so it can be deduplicated.
                *last_text.borrow_mut() = text;
                push_event(egui::Event::Ime(egui::ImeEvent::Commit(commit_text)));
            });
        }

        {
            let composing = composing.clone();
            let suppress_composition_end = suppress_composition_end.clone();
            let preedit_text_state = preedit_text.clone();
            let last_text = last_text.clone();
            let input_for_event = input.clone();
            add_listener(&input, "input", &mut listeners, move |event| {
                let Ok(event) = event.dyn_into::<web_sys::InputEvent>() else {
                    return;
                };
                let input_type = event.input_type();
                let is_composing = composing.get() || event.is_composing();
                let text = input_for_event.value();
                if is_composing {
                    if suppress_composition_end.get() {
                        return;
                    }
                    let previous = last_text.borrow().clone();
                    let prefix_len = longest_common_prefix_length(&text, &previous);
                    let previous_len = previous.chars().count();
                    if prefix_len < previous_len {
                        push_event(egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                            before_chars: previous_len - prefix_len,
                            after_chars: 0,
                        }));
                    }
                    let preedit_text = text.chars().skip(prefix_len).collect::<String>();
                    *preedit_text_state.borrow_mut() = preedit_text.clone();
                    *last_text.borrow_mut() = text.chars().take(prefix_len).collect();
                    push_event(egui::Event::Ime(egui::ImeEvent::Preedit {
                        text: preedit_text,
                        active_range_chars: None,
                    }));
                    return;
                }
                match input_type.as_str() {
                    "insertText" | "insertReplacementText" => {
                        let previous = last_text.borrow().clone();
                        let prefix_len = longest_common_prefix_length(&text, &previous);
                        let previous_len = previous.chars().count();
                        if prefix_len < previous_len {
                            push_event(egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                                before_chars: previous_len - prefix_len,
                                after_chars: 0,
                            }));
                        }
                        let inserted_text = text.chars().skip(prefix_len).collect::<String>();
                        if !inserted_text.is_empty() {
                            push_event(egui::Event::Text(inserted_text));
                        }
                    }
                    "deleteContentBackward" => {
                        push_event(egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                            before_chars: 1,
                            after_chars: 0,
                        }));
                    }
                    "deleteContentForward" => {
                        push_event(egui::Event::Ime(egui::ImeEvent::DeleteSurrounding {
                            before_chars: 0,
                            after_chars: 1,
                        }));
                    }
                    _ => {}
                }
                input_for_event.set_value("");
                preedit_text_state.borrow_mut().clear();
                last_text.borrow_mut().clear();
            });
        }

        {
            let composing = composing.clone();
            add_listener(&input, "keydown", &mut listeners, move |event| {
                let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>() else {
                    return;
                };
                if composing.get() || event.is_composing() || event.key_code() == 229 {
                    return;
                }
                forward_key(&event, true);
            });
        }

        add_listener(&input, "keyup", &mut listeners, move |event| {
            let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>() else {
                return;
            };
            if !event.is_composing() && event.key_code() != 229 {
                forward_key(&event, false);
            }
        });

        {
            let focused = focused.clone();
            add_listener(&input, "focus", &mut listeners, move |_| {
                focused.set(true);
            });
        }
        {
            let focused = focused.clone();
            let ime_enabled = ime_enabled.clone();
            let input_for_blur = input.clone();
            add_listener(&input, "blur", &mut listeners, move |_| {
                focused.set(false);
                if ime_enabled.get() {
                    // Restore focus during the browser's blur handling so mobile keyboards stay available.
                    let _ = input_for_blur.focus();
                }
            });
        }

        *slot.borrow_mut() = Some(WebInput {
            input,
            _listeners: listeners,
            focused,
            ime_enabled,
            composing,
            suppress_composition_end,
            preedit_text,
            last_text,
        });
    });
}

pub(crate) fn take_events() -> Vec<egui::Event> {
    EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
}

pub(crate) fn commit_preedit_before_pointer() -> Option<egui::Event> {
    INPUT.with(|slot| {
        let slot = slot.borrow();
        let input = slot.as_ref()?;
        let preedit = std::mem::take(&mut *input.preedit_text.borrow_mut());
        if preedit.is_empty() {
            return None;
        }
        // Commit before pointer events so egui applies it at the composition's original cursor.
        input.composing.set(false);
        input.suppress_composition_end.set(true);
        *input.last_text.borrow_mut() = input.input.value();
        Some(egui::Event::Ime(egui::ImeEvent::Commit(preedit)))
    })
}

pub(crate) fn update_focus(ime_enabled: bool) {
    INPUT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(input) = slot.as_mut() else { return };
        let was_ime_enabled = input.ime_enabled.replace(ime_enabled);
        if ime_enabled {
            if !input.focused.get() {
                let _ = input.input.focus();
            }
        } else if was_ime_enabled {
            let _ = input.input.blur();
            input.input.set_value("");
            input.preedit_text.borrow_mut().clear();
            input.last_text.borrow_mut().clear();
        }
    });
}

fn longest_common_prefix_length(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(left, right)| left == right)
        .count()
}

fn add_listener(
    input: &HtmlInputElement,
    name: &str,
    listeners: &mut Vec<Closure<dyn FnMut(Event)>>,
    handler: impl FnMut(Event) + 'static,
) {
    let callback = Closure::wrap(Box::new(handler) as Box<dyn FnMut(Event)>);
    if input
        .add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
        .is_ok()
    {
        listeners.push(callback);
    }
}

fn push_event(event: egui::Event) {
    EVENTS.with(|events| events.borrow_mut().push(event));
}

fn forward_key(event: &web_sys::KeyboardEvent, pressed: bool) {
    let modifiers = egui::Modifiers {
        alt: event.alt_key(),
        ctrl: event.ctrl_key(),
        shift: event.shift_key(),
        mac_cmd: event.meta_key(),
        command: event.ctrl_key() || event.meta_key(),
    };
    push_event(egui::Event::ModifiersChanged(modifiers));
    let logical_key = egui::Key::from_name(&event.key());
    let physical_key = egui::Key::from_name(&event.code());

    if let Some(key) = logical_key.or(physical_key) {
        if pressed && modifiers.command {
            match key {
                egui::Key::C => push_event(egui::Event::Copy),
                egui::Key::X => push_event(egui::Event::Cut),
                egui::Key::V => {
                    event.prevent_default();
                    if let Some(clipboard) =
                        web_sys::window().map(|window| window.navigator().clipboard())
                    {
                        let promise = clipboard.read_text();
                        wasm_bindgen_futures::spawn_local(async move {
                            if let Ok(value) = wasm_bindgen_futures::JsFuture::from(promise).await {
                                if let Some(text) = value.as_string() {
                                    push_event(egui::Event::Paste(text));
                                }
                            }
                        });
                    }
                }
                _ => {}
            }
        }

        if pressed
            && matches!(
                key,
                egui::Key::Tab
                    | egui::Key::Backspace
                    | egui::Key::ArrowDown
                    | egui::Key::ArrowLeft
                    | egui::Key::ArrowRight
                    | egui::Key::ArrowUp
            )
        {
            event.prevent_default();
        }

        push_event(egui::Event::Key {
            key,
            physical_key,
            pressed,
            repeat: pressed && event.repeat(),
            modifiers,
        });
    }
}
