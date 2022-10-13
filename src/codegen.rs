use html5ever::tokenizer::{Tokenizer, TokenizerOpts, BufferQueue};
use proc_macro::TokenTree;
use string_tools::get_all_between_strict;
use crate::*;

fn attr_to_yew_string((name, value): (String, String), opts: &mut Vec<String>, iters: &mut Vec<String>, args: &Args) -> String {
    if name == "opt" || name == "iter" || name == "present-if" {
        return String::new()
    }
    if value.starts_with('[') && value.ends_with(']') && !value[1..value.len() - 1].chars().any(|c| !c.is_ascii_alphanumeric() && c != '_') {
        let id = value[1..value.len() - 1].to_string();
        match args.get_val(&id, opts, iters, args) {
            TokenTree::Literal(lit) => {
                let mut value = lit.to_string();
                if (value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')) {
                    value = value[1..value.len() - 1].to_string();
                }
                format!("{name}=\"{value}\"")
            },
            TokenTree::Ident(ident) if ident.to_string() == "true" || ident.to_string() == "false" => format!("{name}=\"{ident}\""),
            value => {
                format!("{name}={{{value}}}")
            }
        }
    } else if value.contains('[') && value.contains(']') {
        let mut text = value;
        let mut end = Vec::new();
        let mut i = 0;
        while let Some(to_replace) = get_all_between_strict(&text, "[", "]").map(|s| s.to_string()) {
            let mut value = args.get_val(&to_replace, opts, iters, args).to_string();
            if to_replace.starts_with("opt_") || to_replace.ends_with("_opt") || to_replace.starts_with("iter_") || to_replace.ends_with("_iter") {
                value = format!("macro_produced_{to_replace}");
            };
    
            if (value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')) {
                value = value[1..value.len() - 1].to_string();
                text = text.replace(&format!("[{}]", to_replace), &value);
            } else {
                text = text.replace(&format!("[{}]", to_replace), &format!("{{attribute_value_{i}}}"));
                end.push(format!("attribute_value_{i} = {value}"));
                i += 1;
            }
        }
        match end.is_empty() {
            true => format!("{name}=\"{text}\""),
            false => format!("{name}={{ format!(\"{text}\", {}) }}", end.join(", "))
        }
    } else {
        format!("{}=\"{}\"", name, value)
    }
}

fn html_part_to_yew_string(part: HtmlPart, depth: usize, opts: &mut Vec<String>, iters: &mut Vec<String>, args: &Args) -> String {
    let tabs = "    ".repeat(depth);
    match part {
        HtmlPart::Element(el) => {
            if el.self_closing && !el.close_attrs.is_empty() {
                abort!(args.path_span, "Self-closing tags cannot have closing attributes");
            }
            if el.self_closing && !el.children.is_empty() {
                abort!(args.path_span, "Self-closing tags cannot have children");
            }
            let opt = el.open_attrs.iter().any(|(n,_)| n=="opt");
            let iter = el.open_attrs.iter().any(|(n,_)| n=="iter");
            let present_if = el.open_attrs.iter().find(|(n,_)| n=="present-if").map(|(_,v)| v.to_owned());

            let mut inner_opts = Vec::new();
            let mut inner_iters = Vec::new();
            let mut f_open_attrs = el.open_attrs.into_iter().map(|a| attr_to_yew_string(a, &mut inner_opts, &mut inner_iters, args)).collect::<Vec<_>>().join(" ");
            if !f_open_attrs.is_empty() {
                f_open_attrs.insert(0, ' ');
            }
            let mut f_close_attrs = el.close_attrs.into_iter().map(|a| attr_to_yew_string(a, &mut inner_opts, &mut inner_iters, args)).collect::<Vec<_>>().join(" ");
            if !f_close_attrs.is_empty() {
                f_close_attrs.insert(0, ' ');
            }
            let name = el.name;
            let mut content = el.children.into_iter().map(|p| html_part_to_yew_string(p, depth + 1, &mut inner_opts, &mut inner_iters, args)).collect::<Vec<_>>().join("");
            inner_opts.sort();
            inner_opts.dedup();
            inner_iters.sort();
            inner_iters.dedup();

            content = match el.self_closing {
                true if &name == "br" => format!("<{name} {f_open_attrs}/>"),
                true => format!("\n{tabs}<{name}{f_open_attrs}/>"),
                false => format!("\n{tabs}<{name}{f_open_attrs}>{content}\n{tabs}</{name}{f_close_attrs}>"),
            };

            match opt {
                true => {
                    let left = inner_opts.iter().map(|id| format!("Some(macro_produced_{id})")).collect::<Vec<_>>().join(", ");
                    let right = inner_opts.iter().map(|id| args.get_val(id, &mut Vec::new(), &mut Vec::new(), args).to_string()).collect::<Vec<_>>().join(", ");
                    content = content.replace('\n', "\n    ");
                    content = format!("\n{tabs}if let ({left}) = ({right}) {{ {content}\n{tabs}}}");
                },
                false => opts.extend_from_slice(&inner_opts),
            }

            match iter {
                true => {
                    let before = inner_iters
                        .iter()
                        .map(|id| format!("let mut macro_produced_{id} = {};", args.get_val(id, &mut Vec::new(), &mut Vec::new(), args)))
                        .collect::<Vec<_>>()
                        .join("");
                    let left = inner_iters.iter().map(|id| format!("Some(macro_produced_{id})")).collect::<Vec<_>>().join(", ");
                    let right = inner_iters.iter().map(|id| format!("macro_produced_{id}.next()", )).collect::<Vec<_>>().join(", ");
                    content = content.replace('\n', "\n        ");
                    content = format!("\n\
                        {tabs}{{{{\n\
                        {tabs}{before}\n\
                        {tabs}let mut fragments = Vec::new();\n\
                        {tabs}while let ({left}) = ({right}) {{\n\
                        {tabs}    fragments.push(yew::html! {{ <> {content} \n\
                        {tabs}    </> }});\n\
                        {tabs}}}\n\
                        {tabs}fragments.into_iter().collect::<yew::Html>()\n\
                        {tabs}}}}}"
                    );
                },
                false => iters.extend_from_slice(&inner_iters),
            }

            if let Some(mut present_if) = present_if {
                let not = present_if.starts_with('!');
                if not {
                    present_if = present_if[1..].to_string();
                }
                let negation = if not {"!"} else {""};
                if !present_if.starts_with('[') || !present_if.ends_with(']') {
                    abort!(args.path_span, "present_if attribute must be a variable");
                }
                present_if = present_if[1..present_if.len()-1].to_string();
                let val = args.get_val(&present_if, &mut Vec::new(), &mut Vec::new(), args);
                content = content.replace('\n', "\n    ");
                content = format!("\n\
                    {tabs}if {negation}{{{val}}} {{\
                    {tabs}{content}\n\
                    {tabs}}}"
                );
            }

            content
        }
        HtmlPart::Text(mut text) => {
            while let Some(to_replace) = get_all_between_strict(&text, "[", "]").map(|s| s.to_string()) {
                let mut value = args.get_val(&to_replace, opts, iters, args).to_string();
                if to_replace.starts_with("opt_") || to_replace.ends_with("_opt") || to_replace.starts_with("iter_") || to_replace.ends_with("_iter") {
                    value = format!("macro_produced_{to_replace}");
                };
                text = text.replace(&format!("[{}]", to_replace), &format!("\"}}{{{value}}}{{\""));
            }
            text = format!("\n{tabs}{{\"{}\"}}", text);
            text = text.replace("{\"\"}", "");

            text
        }
    }
}

pub(crate) fn generate_code(args: Args) -> String {
    let template = match std::fs::read_to_string(&args.path) {
        Ok(template) => template,
        Err(e) => abort!(args.path_span, "Failed to read template file at {}: {}", args.path, e),
    };
    let mut html_parts = Vec::new();
    let html_sink = HtmlSink { html_parts: &mut html_parts, opened_elements: Vec::new(), args: &args };
    let mut html_tokenizer = Tokenizer::new(html_sink, TokenizerOpts::default());
    let mut buffer_queue = BufferQueue::new();
    buffer_queue.push_back(template.into());
    let _  = html_tokenizer.feed(&mut buffer_queue);
    html_tokenizer.end();
    let mut root = Element {
        name: "".to_string(),
        open_attrs: Vec::new(),
        close_attrs: Vec::new(),
        self_closing: false,
        children: html_parts,
    };
    root.clean_text();
    println!("{:#?}", root);

    let yew_html = html_part_to_yew_string(HtmlPart::Element(root), 0, &mut Vec::new(), &mut Vec::new(), &args);
    let yew_code = format!("yew::html! {{ {yew_html} }}");
    println!("{}", yew_code);

    yew_code
}
