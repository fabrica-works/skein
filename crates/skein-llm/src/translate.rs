//! Bridge between the provider-neutral vocabulary and Codex documents.
//! No state or owner references are retained. Measurement and encoding borrow
//! schemas/replay into the reserved bytes (llm.md, sections 4.2, 4.5 and 4.7).
use crate::{Block, Error, Failure, Prompt, Provider, Replay, Role, Stop, Usage, openai};
use alloc::boxed::Box;
use skein_json::{Kind, writer};
#[cfg(test)]
use skein_lib::{List, Writer, bytes};

const TOOL_ERROR: &[u8] = b"Error: ";

/// Measures the borrowed vocabulary without constructing a native request.
pub(crate) fn measure_request(prompt: &Prompt, limits: &openai::Limits) -> Result<u32, Error> {
    validate_codex(prompt, limits)?;
    let bounded = writer::Limits { depth: limits.depth, length: limits.request };
    let mut out = writer::Encoder::measure(&bounded);
    write_request(&mut out, prompt)?;
    match openai::measured(out, bounded, crate::Cap::Request) {
        Ok(length) => Ok(length),
        Err(error) => Err(decode(error)),
    }
}

/// Writes directly into the reserved body; schemas, text and replay stay borrowed.
pub(crate) fn encode_request(prompt: &Prompt, limits: &openai::Limits) -> Result<Box<[u8]>, Error> {
    let length = measure_request(prompt, limits)?;
    let mut out = writer::Encoder::write(length, &writer::Limits { depth: limits.depth, length: limits.request });
    write_request(&mut out, prompt)?;
    Ok(out.finish())
}

fn validate_codex(prompt: &Prompt, limits: &openai::Limits) -> Result<(), Error> {
    if prompt.max_output_tokens.is_some() {
        return Err(Error::Unsupported);
    }
    validate(prompt, Provider::OpenAiCodex, limits)
}

fn write_request(out: &mut writer::Encoder, prompt: &Prompt) -> Result<(), Error> {
    out.object_start();
    out.key(b"model");
    out.string(&prompt.model);
    out.key(b"instructions");
    out.string(&prompt.instructions);
    out.key(b"stream");
    out.boolean(true);
    out.key(b"store");
    out.boolean(false);
    out.key(b"include");
    out.array_start();
    out.string(b"reasoning.encrypted_content");
    out.array_end();
    out.key(b"tools");
    out.array_start();
    for tool in &prompt.tools {
        out.object_start();
        out.key(b"type");
        out.string(b"function");
        out.key(b"name");
        out.string(&tool.name);
        out.key(b"description");
        out.string(&tool.description);
        out.key(b"parameters");
        tool.schema.write(out);
        out.key(b"strict");
        out.boolean(false);
        out.object_end();
    }
    out.array_end();
    out.key(b"tool_choice");
    out.string(match &prompt.choice {
        crate::ToolChoice::Auto | crate::ToolChoice::Only(_) => b"auto",
        crate::ToolChoice::None => b"none",
    });
    out.key(b"parallel_tool_calls");
    out.boolean(true);
    out.key(b"input");
    out.array_start();
    for message in &prompt.messages {
        for block in &message.content {
            write_block(out, block, message.role)?;
        }
    }
    out.array_end();
    if let Some(effort) = &prompt.reasoning_effort {
        out.key(b"reasoning");
        out.object_start();
        out.key(b"effort");
        out.string(effort);
        out.object_end();
    }
    if let Some(affinity) = prompt.affinity {
        out.key(b"prompt_cache_key");
        out.string(&crate::affinity::uuid(&affinity.key));
    }
    out.key(b"text");
    out.object_start();
    out.key(b"verbosity");
    out.string(openai::identity::VERBOSITY);
    out.object_end();
    out.object_end();
    Ok(())
}

fn write_block(out: &mut writer::Encoder, block: &Block, role: Role) -> Result<(), Error> {
    match block {
        Block::Dropped { .. } => {}
        Block::Oversize { .. } | Block::Cut { .. } => return Err(Error::Invalid),
        Block::Reasoning { replay } => write_reasoning(out, replay)?,
        Block::Text { text, replay } | Block::Refusal { text, replay } => {
            let refusal = match block {
                Block::Refusal { .. } => true,
                Block::Text { .. } => false,
                Block::ToolCall { .. }
                | Block::ToolResult { .. }
                | Block::Reasoning { .. }
                | Block::Oversize { .. }
                | Block::Cut { .. }
                | Block::Dropped { .. } => unreachable!("text arm"),
            };
            out.object_start();
            out.key(b"type");
            out.string(b"message");
            out.key(b"role");
            out.string(match role {
                Role::User => b"user",
                Role::Assistant => b"assistant",
            });
            if let Some(id) = replay_text_ref(replay.as_ref(), b"id")? {
                out.key(b"id");
                out.string(id);
            }
            if let Some(phase) = replay_text_ref(replay.as_ref(), b"phase")? {
                out.key(b"phase");
                out.string(phase);
            }
            out.key(b"content");
            out.array_start();
            out.object_start();
            out.key(b"type");
            out.string(match role {
                Role::User => b"input_text",
                Role::Assistant => {
                    if refusal {
                        b"refusal"
                    } else {
                        b"output_text"
                    }
                }
            });
            out.key(if refusal { b"refusal" } else { b"text" });
            out.string(text);
            out.object_end();
            out.array_end();
            out.object_end();
        }
        Block::ToolCall { id, name, arguments, replay } => {
            out.object_start();
            out.key(b"type");
            out.string(b"function_call");
            out.key(b"call_id");
            out.string(id);
            if let Some(item) = replay_text_ref(replay.as_ref(), b"item_id")? {
                out.key(b"id");
                out.string(item);
            }
            out.key(b"name");
            out.string(name);
            out.key(b"arguments");
            out.string(arguments);
            out.object_end();
        }
        Block::ToolResult { id, text, is_error } => {
            out.object_start();
            out.key(b"type");
            out.string(b"function_call_output");
            out.key(b"call_id");
            out.string(id);
            out.key(b"output");
            if *is_error {
                out.string_parts(&[TOOL_ERROR, text]);
            } else {
                out.string(text);
            }
            out.object_end();
        }
    }
    Ok(())
}

fn write_reasoning(out: &mut writer::Encoder, replay: &Replay) -> Result<(), Error> {
    let mut next = 0;
    for _token in 0..openai::json::len(replay.value.view()) {
        let view = match openai::response::reasoning::replay_item(&replay.value, &mut next) {
            Ok(Some(view)) => view,
            Ok(None) => break,
            Err(error) => return Err(decode(error)),
        };
        let kind = match openai::json::required(view, b"type") {
            Ok(at) => match openai::json::value_at(view, at) {
                Ok(kind) => kind,
                Err(error) => return Err(decode(error)),
            },
            Err(error) => return Err(decode(error)),
        };
        match openai::json::text_ref(kind) {
            Ok(b"" | b"message" | b"function_call" | b"function_call_output") => return Err(Error::Invalid),
            Ok(_) => openai::json::write_view(out, view),
            Err(error) => return Err(decode(error)),
        }
    }
    Ok(())
}

fn replay_text_ref<'a>(replay: Option<&'a Replay>, name: &[u8]) -> Result<Option<&'a [u8]>, Error> {
    let Some(replay) = replay else {
        return Ok(None);
    };
    let view = replay.value.view();
    let at = match openai::json::field(view, name) {
        Ok(Some(at)) => at,
        Ok(None) => return Ok(None),
        Err(error) => return Err(decode(error)),
    };
    let value = match openai::json::value_at(view, at) {
        Ok(value) => value,
        Err(error) => return Err(decode(error)),
    };
    if openai::json::len(value) == 1 && openai::json::kind(value, 0) == Some(Kind::Null) {
        Ok(None)
    } else {
        match openai::json::text_ref(value) {
            Ok(text) => Ok(Some(text)),
            Err(error) => Err(decode(error)),
        }
    }
}

#[cfg(test)]
#[expect(clippy::manual_map, reason = "the strict subset excludes closure-taking maps")]
pub(crate) fn request(prompt: &Prompt, provider: Provider, limits: &openai::Limits) -> Result<openai::Request, Error> {
    if provider != Provider::OpenAiCodex || prompt.max_output_tokens.is_some() {
        return Err(Error::Unsupported);
    }
    validate(prompt, provider, limits)?;
    let mut tools = List::with_capacity(limits.tools);
    for tool in &prompt.tools {
        let raw = openai::Tool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            schema: tool.schema.clone(),
        };
        if tools.push(raw).is_err() {
            return Err(Error::limit(crate::Cap::Tools, limits.tools));
        }
    }
    let mut input = List::with_capacity(limits.history_items);
    for message in &prompt.messages {
        for block in &message.content {
            match block {
                Block::Dropped { .. } => continue,
                Block::Text { .. }
                | Block::Refusal { .. }
                | Block::ToolCall { .. }
                | Block::ToolResult { .. }
                | Block::Reasoning { .. }
                | Block::Oversize { .. }
                | Block::Cut { .. } => {}
            }
            let item = input_block(block, message.role)?;
            if input.push(item).is_err() {
                return Err(Error::limit(crate::Cap::HistoryItems, limits.history_items));
            }
        }
    }
    let raw = openai::Request {
        model: prompt.model.clone(),
        instructions: prompt.instructions.clone(),
        tools: tools.into_boxed(),
        input: input.into_boxed(),
        effort: prompt.reasoning_effort.clone(),
        prompt_cache_key: match prompt.affinity {
            Some(affinity) => Some(bytes::copy_of(&crate::affinity::uuid(&affinity.key))),
            None => None,
        },
        choice: prompt.choice.clone(),
    };
    match openai::measure_request(&raw, limits) {
        Ok(_) => Ok(raw),
        Err(error) => Err(decode(error)),
    }
}

fn validate(prompt: &Prompt, provider: Provider, limits: &openai::Limits) -> Result<(), Error> {
    if prompt.model.is_empty() {
        return Err(Error::Invalid);
    }
    if prompt.tools.len() > usize::try_from(limits.tools).expect("bounded tools") {
        return Err(Error::limit(crate::Cap::Tools, limits.tools));
    }
    let count = usize::try_from(limits.history_items).expect("bounded history");
    if prompt.messages.len() > count {
        return Err(Error::limit(crate::Cap::HistoryItems, limits.history_items));
    }
    validate_choice(&prompt.choice, &prompt.tools)?;
    let mut budget: u64 = 0;
    match &prompt.choice {
        crate::ToolChoice::Auto | crate::ToolChoice::None => {}
        crate::ToolChoice::Only(names) => {
            for name in names {
                charge_text(name, &mut budget, limits)?;
            }
        }
    }
    charge_text(&prompt.model, &mut budget, limits)?;
    charge_text(&prompt.instructions, &mut budget, limits)?;
    if let Some(value) = &prompt.reasoning_effort {
        charge_text(value, &mut budget, limits)?;
    }
    if let Some(value) = prompt.affinity {
        charge_text(&crate::affinity::uuid(&value.key), &mut budget, limits)?;
    }
    for tool in &prompt.tools {
        if tool.name.is_empty() || openai::json::kind(tool.schema.view(), 0) != Some(Kind::ObjectStart) {
            return Err(Error::Invalid);
        }
        charge_text(&tool.name, &mut budget, limits)?;
        charge_text(&tool.description, &mut budget, limits)?;
        charge_json(&tool.schema, &mut budget, limits)?;
    }
    let mut blocks: usize = 0;
    for message in &prompt.messages {
        blocks = blocks
            .checked_add(message.content.len())
            .ok_or(Error::limit(crate::Cap::HistoryItems, limits.history_items))?;
        if blocks > count {
            return Err(Error::limit(crate::Cap::HistoryItems, limits.history_items));
        }
        for block in &message.content {
            validate_block(block, message.role, provider, &mut budget, limits)?;
        }
    }
    Ok(())
}
fn validate_block(
    block: &Block,
    role: Role,
    provider: Provider,
    budget: &mut u64,
    limits: &openai::Limits,
) -> Result<(), Error> {
    match block {
        Block::Oversize { .. } | Block::Cut { .. } => return Err(Error::Invalid),
        Block::Dropped { .. } => {}
        Block::Text { text, replay } => {
            charge_text(text, budget, limits)?;
            charge_replay(replay.as_ref(), provider, budget, limits, limits.metadata, crate::Cap::Metadata)?;
            metadata(replay.as_ref(), b"id", b"phase")?;
            if replay.is_some() && role != Role::Assistant {
                return Err(Error::Invalid);
            }
        }
        Block::Refusal { text, replay } => {
            if role != Role::Assistant {
                return Err(Error::Invalid);
            }
            charge_text(text, budget, limits)?;
            charge_replay(replay.as_ref(), provider, budget, limits, limits.metadata, crate::Cap::Metadata)?;
            metadata(replay.as_ref(), b"id", b"phase")?;
        }
        Block::ToolCall { id, name, arguments, replay } => {
            if role != Role::Assistant || id.is_empty() || name.is_empty() {
                return Err(Error::Invalid);
            }
            charge_text(id, budget, limits)?;
            charge_text(name, budget, limits)?;
            charge_text(arguments, budget, limits)?;
            charge_replay(replay.as_ref(), provider, budget, limits, limits.metadata, crate::Cap::Metadata)?;
            metadata(replay.as_ref(), b"item_id", b"item_id")?;
        }
        Block::ToolResult { id, text, is_error } => {
            if role != Role::User || id.is_empty() {
                return Err(Error::Invalid);
            }
            charge_text(id, budget, limits)?;
            charge_text(text, budget, limits)?;
            if *is_error {
                charge(TOOL_ERROR.len(), budget, limits)?;
            }
        }
        Block::Reasoning { replay } => {
            if role != Role::Assistant {
                return Err(Error::Invalid);
            }
            charge_replay(Some(replay), provider, budget, limits, limits.reasoning, crate::Cap::Reasoning)?;
        }
    }
    Ok(())
}

/// The offered list stays intact; Only is local policy, not client filtering.
pub(crate) fn validate_choice(choice: &crate::ToolChoice, tools: &[crate::Tool]) -> Result<(), Error> {
    match choice {
        crate::ToolChoice::Auto | crate::ToolChoice::None => Ok(()),
        crate::ToolChoice::Only(names) => {
            if names.is_empty() {
                return Err(Error::Invalid);
            }
            for (index, name) in names.iter().enumerate() {
                let mut offered = false;
                for tool in tools {
                    offered |= tool.name == *name;
                }
                if !offered {
                    return Err(Error::Invalid);
                }
                for previous in names.get(..index).expect("an index through the names") {
                    if previous == name {
                        return Err(Error::Invalid);
                    }
                }
            }
            Ok(())
        }
    }
}

fn charge_text(value: &[u8], budget: &mut u64, limits: &openai::Limits) -> Result<(), Error> {
    let bounded = writer::Limits { depth: limits.depth, length: limits.request };
    let mut text = writer::Encoder::measure(&bounded);
    text.string(value);
    match text.measured() {
        Ok(_) => charge(value.len(), budget, limits),
        Err(writer::Refusal::Text | writer::Refusal::Number) => Err(Error::Invalid),
        Err(writer::Refusal::TooLong) => Err(Error::limit(crate::Cap::Request, limits.request)),
        Err(writer::Refusal::TooDeep) => Err(Error::limit(crate::Cap::Depth, limits.depth)),
    }
}
fn charge(size: usize, budget: &mut u64, limits: &openai::Limits) -> Result<(), Error> {
    *budget = budget
        .checked_add(u64::try_from(size).expect("usize fits u64"))
        .ok_or(Error::limit(crate::Cap::Request, limits.request))?;
    if *budget > u64::from(limits.request) {
        return Err(Error::limit(crate::Cap::Request, limits.request));
    }
    Ok(())
}
fn charge_json(value: &openai::Json, budget: &mut u64, limits: &openai::Limits) -> Result<(), Error> {
    let mut measure = writer::Encoder::measure(&writer::Limits { depth: limits.depth, length: limits.request });
    let view = value.view();
    for index in 0..openai::json::len(view) {
        charge(1, budget, limits)?;
        match openai::json::kind(view, index).expect("admitted record") {
            Kind::Key | Kind::String | Kind::Number => {
                charge_text(openai::json::record_text(view, index).expect("admitted text"), budget, limits)?;
            }
            Kind::ObjectStart
            | Kind::ObjectEnd
            | Kind::ArrayStart
            | Kind::ArrayEnd
            | Kind::True
            | Kind::False
            | Kind::Null => {}
            Kind::Long => unreachable!("admitted Json contains no Long"),
        }
    }
    value.write(&mut measure);
    match measure.measured() {
        Ok(_) => Ok(()),
        Err(writer::Refusal::TooLong) => Err(Error::limit(crate::Cap::Request, limits.request)),
        Err(writer::Refusal::TooDeep) => Err(Error::limit(crate::Cap::Depth, limits.depth)),
        Err(writer::Refusal::Text | writer::Refusal::Number) => Err(Error::Invalid),
    }
}
fn charge_replay(
    replay: Option<&Replay>,
    provider: Provider,
    budget: &mut u64,
    limits: &openai::Limits,
    maximum: u32,
    cap: crate::Cap,
) -> Result<(), Error> {
    if let Some(replay) = replay {
        if replay.provider != provider {
            return Err(Error::Unsupported);
        }
        let mut next = 0;
        for _token in 0..openai::json::len(replay.value.view()) {
            let view = match openai::response::reasoning::replay_item(&replay.value, &mut next) {
                Ok(Some(view)) => view,
                Ok(None) => break,
                Err(error) => return Err(decode(error)),
            };
            charge_replay_item(view, provider, maximum, cap, limits)?;
        }
        charge_json(&replay.value, budget, limits)?;
    }
    Ok(())
}
// A replay envelope is tied to the block that owns it. Unknown envelope
// fields cannot be silently discarded; reasoning documents remain opaque.
fn metadata(replay: Option<&Replay>, first: &[u8], second: &[u8]) -> Result<(), Error> {
    let Some(replay) = replay else {
        return Ok(());
    };
    let view = replay.value.view();
    for index in 0..openai::json::len(view) {
        match openai::json::kind(view, index).expect("admitted record") {
            Kind::Key => {
                let name = openai::json::record_text(view, index).expect("admitted key");
                if name != first && name != second {
                    return Err(Error::Invalid);
                }
            }
            Kind::ObjectStart
            | Kind::ObjectEnd
            | Kind::ArrayStart
            | Kind::ArrayEnd
            | Kind::String
            | Kind::Number
            | Kind::True
            | Kind::False
            | Kind::Null => {}
            Kind::Long => unreachable!("admitted Json contains no Long"),
        }
    }
    for name in [first, second] {
        let tokens = replay.value.view();
        let field = match openai::json::field(tokens, name) {
            Ok(at) => at,
            Err(error) => return Err(decode(error)),
        };
        if let Some(at) = field {
            match openai::json::value_at(tokens, at) {
                Ok(value) => {
                    if openai::json::len(value) != 1 {
                        return Err(Error::Invalid);
                    }
                    match openai::json::kind(value, 0) {
                        Some(Kind::String | Kind::Null) => {}
                        Some(
                            Kind::ObjectStart
                            | Kind::ObjectEnd
                            | Kind::ArrayStart
                            | Kind::ArrayEnd
                            | Kind::Key
                            | Kind::Number
                            | Kind::True
                            | Kind::False
                            | Kind::Long,
                        )
                        | None => return Err(Error::Invalid),
                    }
                }
                Err(error) => return Err(decode(error)),
            }
        }
    }
    Ok(())
}
#[cfg(test)]
fn input_block(block: &Block, role: Role) -> Result<openai::Input, Error> {
    match block {
        Block::Oversize { .. } | Block::Cut { .. } | Block::Dropped { .. } => Err(Error::Invalid),
        Block::Text { text, replay } | Block::Refusal { text, replay } => {
            let id = replay_text(replay.as_ref(), b"id")?;
            let phase = replay_text(replay.as_ref(), b"phase")?;
            let refusal = match block {
                Block::Refusal { .. } => true,
                Block::Text { .. } => false,
                Block::ToolCall { .. }
                | Block::ToolResult { .. }
                | Block::Reasoning { .. }
                | Block::Oversize { .. }
                | Block::Cut { .. }
                | Block::Dropped { .. } => {
                    unreachable!("text variants entered this arm")
                }
            };
            Ok(openai::Input::Message {
                role: match role {
                    Role::User => openai::Role::User,
                    Role::Assistant => openai::Role::Assistant,
                },
                text: text.clone(),
                id,
                phase,
                refusal,
            })
        }
        Block::ToolCall { id, name, arguments, replay } => Ok(openai::Input::FunctionCall {
            call_id: id.clone(),
            item_id: replay_text(replay.as_ref(), b"item_id")?,
            name: name.clone(),
            arguments: arguments.clone(),
        }),
        Block::ToolResult { id, text, is_error } => {
            let output = if *is_error {
                let len = text.len().checked_add(TOOL_ERROR.len()).ok_or(Error::Invalid)?;
                let mut output = Writer::new(len);
                output.put(TOOL_ERROR).expect("the error prefix was measured");
                output.put(text).expect("the error result was measured");
                output.finish()
            } else {
                text.clone()
            };
            Ok(openai::Input::FunctionOutput { call_id: id.clone(), output })
        }
        Block::Reasoning { replay } => {
            let bounded = writer::Limits { depth: u32::MAX, length: u32::MAX };
            let mut measure = writer::Encoder::measure(&bounded);
            measure.array_start();
            write_reasoning(&mut measure, replay)?;
            measure.array_end();
            Ok(openai::Input::Opaque { value: replay.value.clone() })
        }
    }
}
fn charge_replay_item(
    view: (&skein_json::Document, openai::json::Span),
    provider: Provider,
    maximum: u32,
    cap: crate::Cap,
    limits: &openai::Limits,
) -> Result<(), Error> {
    if openai::json::kind(view, 0) != Some(Kind::ObjectStart) {
        return Err(Error::Invalid);
    }
    let known_reasoning = if cap == crate::Cap::Reasoning && provider == Provider::OpenAiCodex {
        let kind = match openai::json::field(view, b"type") {
            Ok(kind) => kind,
            Err(error) => return Err(decode(error)),
        };
        match openai::json::optional_at(view, kind) {
            Ok(Some(kind)) => match openai::json::text_ref(kind) {
                Ok(kind) => kind == b"reasoning",
                Err(error) => return Err(decode(error)),
            },
            Ok(None) => false,
            Err(error) => return Err(decode(error)),
        }
    } else {
        false
    };
    if known_reasoning {
        let encrypted = match openai::json::field(view, b"encrypted_content") {
            Ok(encrypted) => encrypted,
            Err(error) => return Err(decode(error)),
        };
        match openai::json::optional_at(view, encrypted) {
            Ok(Some(encrypted)) => match openai::json::text_ref(encrypted) {
                Ok(text) if text.len() > usize::try_from(maximum).expect("bounded reasoning") => {
                    return Err(Error::limit(cap, maximum));
                }
                Ok(_) => {}
                Err(error) => return Err(decode(error)),
            },
            Ok(None) => {}
            Err(error) => return Err(decode(error)),
        }
    } else {
        let bounded = writer::Limits { depth: limits.depth, length: maximum };
        let mut measure = writer::Encoder::measure(&bounded);
        openai::json::write_view(&mut measure, view);
        match measure.measured() {
            Ok(_) => {}
            Err(writer::Refusal::TooLong) => return Err(Error::limit(cap, maximum)),
            Err(writer::Refusal::TooDeep) => return Err(Error::limit(crate::Cap::Depth, limits.depth)),
            Err(writer::Refusal::Text | writer::Refusal::Number) => return Err(Error::Invalid),
        }
    }
    Ok(())
}

#[cfg(test)]
fn replay_text(replay: Option<&Replay>, name: &[u8]) -> Result<Option<Box<[u8]>>, Error> {
    let Some(replay) = replay else {
        return Ok(None);
    };
    let tokens = replay.value.view();
    let field = match openai::json::field(tokens, name) {
        Ok(at) => at,
        Err(error) => return Err(decode(error)),
    };
    let Some(at) = field else {
        return Ok(None);
    };
    match openai::json::value_at(tokens, at) {
        Ok(value) => {
            if openai::json::len(value) == 1 && openai::json::kind(value, 0) == Some(Kind::Null) {
                Ok(None)
            } else {
                match openai::json::text(value) {
                    Ok(text) => Ok(Some(text)),
                    Err(error) => Err(decode(error)),
                }
            }
        }
        Err(error) => Err(decode(error)),
    }
}

pub(crate) fn part(provider: Provider, value: openai::Part, limits: &openai::Limits) -> Result<Block, Error> {
    match value {
        openai::Part::Dropped { bytes } => Ok(Block::Dropped { bytes }),
        openai::Part::Text { id, phase, text, refusal } => {
            let replay = Some(replay(provider, b"id", &id, b"phase", phase.as_deref(), limits)?);
            if refusal { Ok(Block::Refusal { text, replay }) } else { Ok(Block::Text { text, replay }) }
        }
        openai::Part::Opaque { bytes } => {
            let value = match openai::Json::from_bytes(&bytes, &limits.document()) {
                Ok(value) => value,
                Err(error) => return Err(decode(error)),
            };
            Ok(Block::Reasoning { replay: Replay { provider, value } })
        }
        openai::Part::ToolCall { call_id, item_id, name, input, too_large, bytes, cut } => {
            if too_large {
                return Ok(Block::Oversize { id: call_id, name, bytes });
            }
            if cut {
                return Ok(Block::Cut { id: call_id, name, arguments: input });
            }
            let replay = Some(replay(provider, b"item_id", &item_id, b"", None, limits)?);
            Ok(Block::ToolCall { id: call_id, name, arguments: input, replay })
        }
    }
}
fn replay(
    provider: Provider,
    first: &[u8],
    value: &[u8],
    second: &[u8],
    optional: Option<&[u8]>,
    limits: &openai::Limits,
) -> Result<Replay, Error> {
    // Synthesized text/refusal/tool metadata must obey the same raw replay
    // cap as native opaque blocks. Json admission measures escaped bytes
    // before emission; the measuring pass allocates no serialized byte buffer.
    let bounded = limits.metadata_document();
    let mut measure = writer::Encoder::measure(&bounded.writer_limits());
    write_replay(&mut measure, first, value, second, optional);
    let length = match measure.measured() {
        Ok(length) => length,
        Err(writer::Refusal::TooLong) => return Err(Error::limit(crate::Cap::Metadata, limits.metadata)),
        Err(writer::Refusal::TooDeep) => return Err(Error::limit(crate::Cap::Depth, limits.depth)),
        Err(writer::Refusal::Text | writer::Refusal::Number) => return Err(Error::Invalid),
    };
    let mut out = writer::Encoder::write(length, &bounded.writer_limits());
    write_replay(&mut out, first, value, second, optional);
    match openai::Json::from_bytes(&out.finish(), &bounded) {
        Ok(value) => Ok(Replay { provider, value }),
        Err(openai::DecodeError::TooLarge { which: crate::Cap::Retained, bound }) => {
            Err(Error::Limit { which: crate::Cap::Metadata, bound })
        }
        Err(error) => Err(decode(error)),
    }
}

fn write_replay(out: &mut writer::Encoder, first: &[u8], value: &[u8], second: &[u8], optional: Option<&[u8]>) {
    out.object_start();
    out.key(first);
    out.string(value);
    if let Some(value) = optional {
        out.key(second);
        out.string(value);
    }
    out.object_end();
}

/// Classifies a bounded document refusal without starting or driving a call.
/// Callers retain `TooLarge` as `Error::Limit`; malformed grammar, missing
/// fields and wrong types become `Error::Invalid`. This does not emit a Client
/// terminal or add retry policy. See `docs/design/llm.md`, Vocabulary and ownership.
#[must_use]
pub const fn decode(error: crate::DocumentError) -> Error {
    match error {
        openai::DecodeError::TooLarge { which, bound } => Error::Limit { which, bound },
        openai::DecodeError::Malformed | openai::DecodeError::Missing | openai::DecodeError::WrongType => {
            Error::Invalid
        }
    }
}

pub(crate) const fn stop(value: openai::Stop) -> Stop {
    match value {
        openai::Stop::EndTurn => Stop::EndTurn,
        openai::Stop::ToolUse => Stop::ToolUse,
        openai::Stop::MaxTokens => Stop::MaxTokens,
        openai::Stop::Refusal => Stop::Refusal,
    }
}
pub(crate) const fn usage(value: openai::Usage) -> Usage {
    Usage {
        input: value.input,
        output: value.output,
        cache_read: value.cache_read,
        cache_write: value.cache_write,
        reasoning: value.reasoning,
    }
}
pub(crate) const fn failure(value: openai::Failure) -> Failure {
    match value {
        openai::Failure::Limit { which, bound } => Failure::Limit { which, bound },
        openai::Failure::Protocol => Failure::Protocol,
        openai::Failure::Unauthorized => Failure::Unauthorized,
        openai::Failure::Exhausted { retry_after } => Failure::Exhausted { retry_after },
        openai::Failure::RateLimited { retry_after } => Failure::RateLimited { retry_after },
        openai::Failure::Overloaded => Failure::Overloaded,
        openai::Failure::Unavailable => Failure::Unavailable,
        openai::Failure::ContextTooLong => Failure::ContextTooLong,
        openai::Failure::Invalid => Failure::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::{failure, part, stop, usage};
    use crate::{Block, Error, Failure, Message, Prompt, Provider, Replay, Role, Stop, Usage, openai};
    use alloc::boxed::Box;
    use skein_lib::{Duration, bytes};
    const LIMITS: openai::Limits = openai::Limits {
        request: 8192,
        retained: 8192,
        strings: 2048,
        depth: 16,
        tokens: 1024,
        output_items: 16,
        input: 2048,
        reasoning: 2048,
        answer: 4096,
        detail_bytes: 256,

        tools: 16,
        history_items: 16,
        metadata: 2048,
        receiving: 1_048_576,
        skip: 8192,
    };
    // The old owned translator is a test oracle only: every successful replay
    // shape exercises the borrowed encoder against its previous exact bytes.
    fn request(prompt: &Prompt, provider: Provider, limits: &openai::Limits) -> Result<openai::Request, Error> {
        let raw = super::request(prompt, provider, limits)?;
        assert_eq!(super::encode_request(prompt, limits)?, openai::encode_request(&raw, limits).unwrap());
        Ok(raw)
    }
    fn prompt(role: Role, block: Block) -> Prompt {
        Prompt {
            model: bytes::copy_of(b"test"),
            instructions: bytes::copy_of(b""),
            tools: Box::new([]),
            messages: Box::new([Message { role, content: Box::new([block]) }]),
            reasoning_effort: None,
            affinity: None,
            choice: crate::ToolChoice::Auto,
            max_output_tokens: None,
        }
    }

    #[test]
    fn drawn_carried_json_matches_the_old_compact_spelling_in_both_dialects() {
        use skein_json::Token;
        use skein_json_world::generate::{self, Shape};
        for seed in 0..60 {
            let mut rng = skein_lib::Rng::new(seed);
            let mut tokens = generate::tokens(&mut rng, Shape { depth: 4, width: 3, string: 8 });
            tokens.insert(0, Token::Key(bytes::copy_of(b"value")));
            tokens.insert(0, Token::ObjectStart);
            tokens.push(Token::ObjectEnd);
            let arguments = generate::render(&mut rng, &tokens).into_boxed_slice();
            let owner = crate::Json::from_bytes(&arguments, &LIMITS.request_document()).unwrap();
            let compact = owner.to_bytes(&LIMITS.request_document()).unwrap();
            let mut input = prompt(
                Role::Assistant,
                Block::ToolCall { id: bytes::copy_of(b"call"), name: bytes::copy_of(b"read"), arguments, replay: None },
            );
            input.tools = Box::new([crate::Tool {
                name: bytes::copy_of(b"read"),
                description: bytes::copy_of(b""),
                schema: owner,
            }]);
            let old_codex = super::request(&input, Provider::OpenAiCodex, &LIMITS).unwrap();
            assert_eq!(
                super::encode_request(&input, &LIMITS).unwrap(),
                openai::encode_request(&old_codex, &LIMITS).unwrap(),
                "Codex seed {seed}: schemas are borrowed, arguments remain text"
            );
            let actual = crate::anthropic::encode_request(&input, 4096, &LIMITS).unwrap();
            input.messages = Box::new([Message {
                role: Role::Assistant,
                content: Box::new([Block::ToolCall {
                    id: bytes::copy_of(b"call"),
                    name: bytes::copy_of(b"read"),
                    arguments: compact,
                    replay: None,
                }]),
            }]);
            assert_eq!(
                actual,
                crate::anthropic::encode_request(&input, 4096, &LIMITS).unwrap(),
                "Anthropic seed {seed}: arguments match the old parse-and-write spelling"
            );
            let reference = crate::Json::from_bytes(&actual, &LIMITS.request_document()).unwrap();
            assert_eq!(actual, reference.to_bytes(&LIMITS.request_document()).unwrap(), "seed {seed}");
        }
    }

    #[test]
    #[expect(
        clippy::disallowed_types,
        clippy::disallowed_methods,
        clippy::disallowed_macros,
        reason = "ordinary fixture code converts archived native values to neutral prompts"
    )]
    fn borrowed_encoder_matches_every_archived_codex_request() {
        for scenario in ["single-text", "tool-call", "tool-result-final"] {
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src/openai/fixtures")
                .join(scenario)
                .join("request.json");
            let captured = std::fs::read(file).unwrap();
            let wrapper = openai::Json::from_bytes(&captured, &LIMITS.document()).unwrap();
            let body = openai::json::text_ref(
                openai::json::value_at(wrapper.view(), openai::json::required(wrapper.view(), b"body").unwrap())
                    .unwrap(),
            )
            .unwrap();
            let native =
                openai::decode_request(&openai::Json::from_bytes(body, &LIMITS.document()).unwrap(), &LIMITS).unwrap();
            let expected = openai::encode_request(&native, &LIMITS).unwrap();
            assert!(native.prompt_cache_key.is_none());
            let tools = native
                .tools
                .into_vec()
                .into_iter()
                .map(|tool| crate::Tool { name: tool.name, description: tool.description, schema: tool.schema })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let messages = native
                .input
                .into_vec()
                .into_iter()
                .map(|input| {
                    let (role, block) = match input {
                        openai::Input::Message { role, text, id, phase, refusal } => {
                            assert!(id.is_none() && phase.is_none());
                            let role = match role {
                                openai::Role::User => Role::User,
                                openai::Role::Assistant => Role::Assistant,
                            };
                            let block = if refusal {
                                Block::Refusal { text, replay: None }
                            } else {
                                Block::Text { text, replay: None }
                            };
                            (role, block)
                        }
                        openai::Input::FunctionCall { call_id, item_id, name, arguments } => {
                            assert!(item_id.is_none());
                            (Role::Assistant, Block::ToolCall { id: call_id, name, arguments, replay: None })
                        }
                        openai::Input::FunctionOutput { call_id, output } => {
                            (Role::User, Block::ToolResult { id: call_id, text: output, is_error: false })
                        }
                        openai::Input::Opaque { value } => (
                            Role::Assistant,
                            Block::Reasoning { replay: Replay { provider: Provider::OpenAiCodex, value } },
                        ),
                    };
                    Message { role, content: Box::new([block]) }
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            let input = Prompt {
                model: native.model,
                instructions: native.instructions,
                tools,
                messages,
                reasoning_effort: native.effort,
                affinity: None,
                choice: native.choice,
                max_output_tokens: None,
            };
            assert_eq!(super::encode_request(&input, &LIMITS).unwrap(), expected, "{scenario}");
        }
    }
    #[test]
    fn text_replays_message_identity_phase_and_refusal() {
        for refusal in [false, true] {
            let block = part(
                Provider::OpenAiCodex,
                openai::Part::Text {
                    id: bytes::copy_of(b"message"),
                    phase: Some(bytes::copy_of(b"commentary")),
                    text: bytes::copy_of(b"answer"),
                    refusal,
                },
                &LIMITS,
            )
            .expect("bounded completed text");
            let raw =
                request(&prompt(Role::Assistant, block), Provider::OpenAiCodex, &LIMITS).expect("assistant replay");
            assert_eq!(
                raw.input.as_ref(),
                &[openai::Input::Message {
                    role: openai::Role::Assistant,
                    text: bytes::copy_of(b"answer"),
                    id: Some(bytes::copy_of(b"message")),
                    phase: Some(bytes::copy_of(b"commentary")),
                    refusal,
                }]
            );
            let wire = openai::encode_request(&raw, &LIMITS).expect("replay wire");
            let value = openai::Json::from_bytes(&wire, &LIMITS.document()).expect("valid document");
            assert_eq!(openai::decode_request(&value, &LIMITS).expect("replay decodes"), raw);
        }
    }
    #[test]
    fn tool_replay_separates_call_and_item_identity() {
        let block = part(
            Provider::OpenAiCodex,
            openai::Part::ToolCall {
                call_id: bytes::copy_of(b"call"),
                item_id: bytes::copy_of(b"item"),
                name: bytes::copy_of(b"read"),
                input: bytes::copy_of(br#"{"path":"a"}"#),
                too_large: false,
                bytes: u64::try_from(bytes::copy_of(br#"{"path":"a"}"#).len()).expect("slice length fits u64"),
                cut: false,
            },
            &LIMITS,
        )
        .expect("bounded tool");
        let raw = request(&prompt(Role::Assistant, block), Provider::OpenAiCodex, &LIMITS).expect("tool replay");
        assert_eq!(
            raw.input.as_ref(),
            &[openai::Input::FunctionCall {
                call_id: bytes::copy_of(b"call"),
                item_id: Some(bytes::copy_of(b"item")),
                name: bytes::copy_of(b"read"),
                arguments: bytes::copy_of(br#"{"path":"a"}"#),
            }]
        );
        let result = Block::ToolResult {
            id: bytes::copy_of(b"call"),
            text: bytes::copy_of(b"permission denied"),
            is_error: true,
        };
        let raw = request(&prompt(Role::User, result), Provider::OpenAiCodex, &LIMITS).expect("tool output replay");
        assert_eq!(
            raw.input.as_ref(),
            &[openai::Input::FunctionOutput {
                call_id: bytes::copy_of(b"call"),
                output: bytes::copy_of(b"Error: permission denied"),
            }]
        );
    }
    #[test]
    fn tool_error_flag_is_visible_on_wire_and_success_is_unchanged() {
        for is_error in [false, true] {
            let block =
                Block::ToolResult { id: bytes::copy_of(b"call"), text: bytes::copy_of(b"result\ntext"), is_error };
            let raw = request(&prompt(Role::User, block), Provider::OpenAiCodex, &LIMITS).expect("tool result");
            let wire = openai::encode_request(&raw, &LIMITS).expect("encoded result");
            let json = openai::Json::from_bytes(&wire, &LIMITS.document()).expect("request JSON");
            let decoded = openai::decode_request(&json, &LIMITS).expect("request replay");
            let output = if is_error { b"Error: result\ntext".as_slice() } else { b"result\ntext".as_slice() };
            assert_eq!(
                decoded.input.as_ref(),
                &[openai::Input::FunctionOutput { call_id: bytes::copy_of(b"call"), output: bytes::copy_of(output) }]
            );
        }
        let mut limits = LIMITS;
        limits.strings = 8;
        let block = Block::ToolResult { id: bytes::copy_of(b"call"), text: bytes::copy_of(b"no"), is_error: true };
        request(&prompt(Role::User, block), Provider::OpenAiCodex, &limits).unwrap();
    }
    #[test]
    fn malformed_tool_arguments_are_preserved_received_and_replayed_as_text() {
        let block = part(
            Provider::OpenAiCodex,
            openai::Part::ToolCall {
                call_id: bytes::copy_of(b"call"),
                item_id: bytes::copy_of(b"item"),
                name: bytes::copy_of(b"read"),
                input: bytes::copy_of(b"broken"),
                too_large: false,
                bytes: u64::try_from(bytes::copy_of(b"broken").len()).expect("slice length fits u64"),
                cut: false,
            },
            &LIMITS,
        )
        .expect("received arguments need not be valid JSON");
        let request = request(&prompt(Role::Assistant, block), Provider::OpenAiCodex, &LIMITS)
            .expect("native arguments are an exact string, including malformed JSON");
        match request.input.as_ref() {
            [openai::Input::FunctionCall { arguments, .. }] => assert_eq!(arguments.as_ref(), b"broken"),
            _ => panic!("one actual native function call"),
        }
    }

    #[test]
    fn reasoning_runs_replay_each_native_item_in_order() {
        let run = br#"[{"id":"r0","type":"reasoning","encrypted_content":"a","summary":[]},{"id":"r1","type":"reasoning","encrypted_content":"b","summary":[]},{"id":"r2","type":"reasoning","encrypted_content":"c","summary":[]}]"#;
        let block = part(Provider::OpenAiCodex, openai::Part::Opaque { bytes: bytes::copy_of(run) }, &LIMITS)
            .expect("bounded run");
        let prompt = prompt(Role::Assistant, block);
        let native = request(&prompt, Provider::OpenAiCodex, &LIMITS).expect("native replay");
        let body = super::encode_request(&prompt, &LIMITS).expect("borrowed replay");
        let offset = bytes::find(&body, run).expect("ordered input");
        assert_eq!(body.get(offset..offset + run.len()), Some(run.as_slice()));
        let native_body = openai::encode_request(&native, &LIMITS).expect("native encoder");
        assert!(bytes::find(&native_body, run).is_some());
    }

    #[test]
    fn a_reasoning_run_checks_each_items_reasoning_limit() {
        let run = br#"[{"id":"r0","type":"reasoning","encrypted_content":"a"},{"id":"r1","type":"reasoning","encrypted_content":"bc"}]"#;
        let block = part(Provider::OpenAiCodex, openai::Part::Opaque { bytes: bytes::copy_of(run) }, &LIMITS)
            .expect("bounded run");
        assert_eq!(
            super::encode_request(&prompt(Role::Assistant, block), &openai::Limits { reasoning: 1, ..LIMITS }),
            Err(Error::Limit { which: crate::Cap::Reasoning, bound: 1 })
        );
    }
    #[test]
    fn encrypted_reasoning_is_exactly_replayed() {
        let value = br#"{"type":"reasoning","id":"r","encrypted_content":"opaque","summary":[]}"#;
        let block = part(Provider::OpenAiCodex, openai::Part::Opaque { bytes: bytes::copy_of(value) }, &LIMITS)
            .expect("bounded reasoning");
        let raw = request(&prompt(Role::Assistant, block), Provider::OpenAiCodex, &LIMITS).expect("reasoning replay");
        assert_eq!(
            raw.input.as_ref(),
            &[openai::Input::Opaque {
                value: openai::Json::from_bytes(value, &LIMITS.document()).expect("reasoning value")
            }]
        );
        let replay = Replay {
            provider: Provider::OpenAiCodex,
            value: openai::Json::from_bytes(br#"{"type":"message"}"#, &LIMITS.document()).expect("JSON object"),
        };
        assert_eq!(
            request(&prompt(Role::Assistant, Block::Reasoning { replay }), Provider::OpenAiCodex, &LIMITS),
            Err(Error::Invalid)
        );
    }
    #[test]
    fn invalid_text_roles_and_bounds_reject_admission() {
        let invalid = Block::Text { text: bytes::copy_of(&[0xff]), replay: None };
        assert_eq!(request(&prompt(Role::User, invalid), Provider::OpenAiCodex, &LIMITS), Err(Error::Invalid));
        let refusal = Block::Refusal { text: bytes::copy_of(b"no"), replay: None };
        assert_eq!(request(&prompt(Role::User, refusal), Provider::OpenAiCodex, &LIMITS), Err(Error::Invalid));
        let text = Block::Text { text: bytes::copy_of(b"long text"), replay: None };
        let mut limits = LIMITS;
        limits.strings = 4;
        request(&prompt(Role::User, text), Provider::OpenAiCodex, &limits).unwrap();
        let mut limits = LIMITS;
        limits.history_items = 0;
        assert_eq!(
            request(
                &prompt(Role::User, Block::Text { text: bytes::copy_of(b""), replay: None }),
                Provider::OpenAiCodex,
                &limits
            ),
            Err(Error::limit(crate::Cap::HistoryItems, 0))
        );
    }
    #[test]
    fn replay_metadata_cannot_be_silently_lost() {
        for value in [
            br#"{"id":"m","unknown":"value"}"#.as_slice(),
            br#"{"id":7}"#.as_slice(),
            br#"{"id":"a","id":"b"}"#.as_slice(),
        ] {
            let replay = Replay {
                provider: Provider::OpenAiCodex,
                value: openai::Json::from_bytes(value, &LIMITS.document()).expect("valid object"),
            };
            let block = Block::Text { text: bytes::copy_of(b"answer"), replay: Some(replay) };
            assert_eq!(request(&prompt(Role::Assistant, block), Provider::OpenAiCodex, &LIMITS), Err(Error::Invalid));
        }
    }
    #[test]
    fn common_terminal_vocabulary_preserves_usage_stop_and_retry() {
        assert_eq!(stop(openai::Stop::Refusal), Stop::Refusal);
        assert_eq!(stop(openai::Stop::MaxTokens), Stop::MaxTokens);
        assert_eq!(stop(openai::Stop::ToolUse), Stop::ToolUse);
        assert_eq!(stop(openai::Stop::EndTurn), Stop::EndTurn);
        let counts = openai::Usage {
            input: Some(10),
            output: Some(3),
            cache_read: Some(5),
            cache_write: Some(2),
            reasoning: None,
        };
        assert_eq!(
            usage(counts),
            Usage { input: Some(10), output: Some(3), cache_read: Some(5), cache_write: Some(2), reasoning: None }
        );
        assert_eq!(
            failure(openai::Failure::RateLimited { retry_after: Duration::from_secs(7) }),
            Failure::RateLimited { retry_after: Duration::from_secs(7) }
        );
    }
    #[test]
    fn only_requires_distinct_offered_names_in_both_dialects() {
        let mut prompt = prompt(Role::User, Block::Text { text: bytes::copy_of(b"hello"), replay: None });
        prompt.tools = Box::new([crate::Tool {
            name: bytes::copy_of(b"read"),
            description: Box::new([]),
            schema: openai::Json::from_bytes(b"{}", &LIMITS.document()).expect("schema object"),
        }]);
        for names in [
            [].as_slice(),
            [bytes::copy_of(b"unknown")].as_slice(),
            [bytes::copy_of(b"read"), bytes::copy_of(b"read")].as_slice(),
        ] {
            prompt.choice = crate::ToolChoice::Only(names.into());
            assert_eq!(request(&prompt, Provider::OpenAiCodex, &LIMITS), Err(Error::Invalid));
            assert_eq!(crate::anthropic::encode_request(&prompt, 4096, &LIMITS), Err(Error::Invalid));
        }
        prompt.choice = crate::ToolChoice::Only(Box::new([bytes::copy_of(b"read")]));
        request(&prompt, Provider::OpenAiCodex, &LIMITS).expect("offered name accepted");
        crate::anthropic::encode_request(&prompt, 4096, &LIMITS).expect("offered name accepted");
    }
    #[test]
    fn oversize_and_cut_are_outcomes_and_neither_can_be_replayed() {
        for block in [
            Block::Oversize { id: bytes::copy_of(b"c"), name: bytes::copy_of(b"read"), bytes: 99 },
            Block::Cut { id: bytes::copy_of(b"c"), name: bytes::copy_of(b"read"), arguments: bytes::copy_of(b"{") },
        ] {
            let prompt = prompt(Role::Assistant, block);
            assert_eq!(request(&prompt, Provider::OpenAiCodex, &LIMITS), Err(Error::Invalid));
            assert_eq!(crate::anthropic::encode_request(&prompt, 4096, &LIMITS), Err(Error::Invalid));
        }
        for cut in [false, true] {
            let native = openai::Part::ToolCall {
                call_id: bytes::copy_of(b"c"),
                item_id: bytes::copy_of(b"i"),
                name: bytes::copy_of(b"read"),
                input: bytes::copy_of(b"{"),
                too_large: false,
                bytes: 1,
                cut,
            };
            let block = part(Provider::OpenAiCodex, native, &LIMITS).expect("refused call is a block");
            if cut {
                assert_eq!(
                    block,
                    Block::Cut {
                        id: bytes::copy_of(b"c"),
                        name: bytes::copy_of(b"read"),
                        arguments: bytes::copy_of(b"{")
                    }
                );
            } else {
                match block {
                    Block::ToolCall { .. } => {}
                    Block::Text { .. }
                    | Block::Refusal { .. }
                    | Block::ToolResult { .. }
                    | Block::Reasoning { .. }
                    | Block::Oversize { .. }
                    | Block::Cut { .. }
                    | Block::Dropped { .. } => unreachable!("complete native call"),
                }
            }
        }
        let native = openai::Part::ToolCall {
            call_id: bytes::copy_of(b"c"),
            item_id: bytes::copy_of(b"i"),
            name: bytes::copy_of(b"read"),
            input: Box::new([]),
            too_large: true,
            bytes: 99,
            cut: false,
        };
        assert_eq!(
            part(Provider::OpenAiCodex, native, &LIMITS),
            Ok(Block::Oversize { id: bytes::copy_of(b"c"), name: bytes::copy_of(b"read"), bytes: 99 })
        );
    }
    #[test]
    fn dropped_history_is_admitted_and_emits_no_codex_item() {
        let mut history = prompt(Role::Assistant, Block::Dropped { bytes: u64::MAX });
        history.messages[0].content =
            Box::new([Block::Dropped { bytes: u64::MAX }, Block::Text { text: bytes::copy_of(b"kept"), replay: None }]);
        let actual = request(&history, Provider::OpenAiCodex, &LIMITS).unwrap();
        let expected = request(
            &prompt(Role::Assistant, Block::Text { text: bytes::copy_of(b"kept"), replay: None }),
            Provider::OpenAiCodex,
            &LIMITS,
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            part(Provider::OpenAiCodex, openai::Part::Dropped { bytes: 99 }, &LIMITS),
            Ok(Block::Dropped { bytes: 99 })
        );
    }
}
