use crate::{
    document::{read_context, DocumentContent},
    parser::ParseSource,
    writer::BufWriter,
    CommentTag, Comments, CommentsRef, Document, Markdown, PreprocessorOutput,
    CONTRACT_INHERITANCE_ID, DEPLOYMENTS_ID, GIT_SOURCE_ID, INHERITDOC_ID,
};
use forge_fmt::solang_ext::SafeUnwrap;
use itertools::Itertools;
use solang_parser::pt::Base;
use std::path::Path;

/// The result of [Asdoc::as_doc] method.
pub type AsDocResult = Result<String, std::fmt::Error>;

/// A trait for formatting a parse unit as documentation.
#[auto_impl::auto_impl(&)]
pub trait AsDoc {
    /// Formats a parse tree item into a doc string.
    fn as_doc(&self) -> AsDocResult;
}

impl AsDoc for String {
    fn as_doc(&self) -> AsDocResult {
        Ok(self.to_owned())
    }
}

impl AsDoc for Comments {
    fn as_doc(&self) -> AsDocResult {
        CommentsRef::from(self).as_doc()
    }
}

impl<'a> AsDoc for CommentsRef<'a> {
    // TODO: support other tags
    fn as_doc(&self) -> AsDocResult {
        let mut writer = BufWriter::default();

        // Write author tag(s)
        let authors = self.include_tag(CommentTag::Author);
        if !authors.is_empty() {
            writer.write_bold(&format!("Author{}:", if authors.len() == 1 { "" } else { "s" }))?;
            writer.writeln_raw(authors.iter().map(|a| &a.value).join(", "))?;
            writer.writeln()?;
        }

        // Write notice tags
        let notices = self.include_tag(CommentTag::Notice);
        for notice in notices.iter() {
            writer.writeln_raw(&notice.value)?;
            writer.writeln()?;
        }

        // Write dev tags
        let devs = self.include_tag(CommentTag::Dev);
        for dev in devs.iter() {
            writer.write_italic(&dev.value)?;
            writer.writeln()?;
        }

        Ok(writer.finish())
    }
}

impl AsDoc for Base {
    fn as_doc(&self) -> AsDocResult {
        Ok(self.name.identifiers.iter().map(|ident| ident.name.to_owned()).join("."))
    }
}

impl AsDoc for Document {
    fn as_doc(&self) -> AsDocResult {
        let mut writer = BufWriter::default();

        match &self.content {
            DocumentContent::OverloadedFunctions(items) => {
                writer
                    .write_title(&format!("function {}", items.first().unwrap().source.ident()))?;
                if let Some(git_source) = read_context!(self, GIT_SOURCE_ID, GitSource) {
                    writer.write_link("Git Source", &git_source)?;
                    writer.writeln()?;
                }

                for item in items.iter() {
                    let func = item.as_function().unwrap();
                    let mut heading = item.source.ident();
                    if !func.params.is_empty() {
                        heading.push_str(&format!(
                            "({})",
                            func.params
                                .iter()
                                .map(|p| p.1.as_ref().map(|p| p.ty.to_string()).unwrap_or_default())
                                .join(", ")
                        ));
                    }
                    writer.write_heading(&heading)?;
                    writer.write_section(&item.comments, &item.code)?;
                }
            }
            DocumentContent::Constants(items) => {
                writer.write_title("Constants")?;
                if let Some(git_source) = read_context!(self, GIT_SOURCE_ID, GitSource) {
                    writer.write_link("Git Source", &git_source)?;
                    writer.writeln()?;
                }

                for item in items.iter() {
                    let var = item.as_variable().unwrap();
                    writer.write_heading(&var.name.safe_unwrap().name)?;
                    writer.write_section(&item.comments, &item.code)?;
                }
            }
            DocumentContent::Single(item) => {
                writer.write_title(&item.source.ident())?;
                if let Some(git_source) = read_context!(self, GIT_SOURCE_ID, GitSource) {
                    writer.write_link("Git Source", &git_source)?;
                    writer.writeln()?;
                }

                if let Some(deployments) = read_context!(self, DEPLOYMENTS_ID, Deployments) {
                    writer.write_deployments_table(deployments)?;
                }

                match &item.source {
                    ParseSource::Contract(contract) => {
                        if !contract.base.is_empty() {
                            writer.write_bold("Inherits:")?;

                            let mut bases = vec![];
                            let linked =
                                read_context!(self, CONTRACT_INHERITANCE_ID, ContractInheritance);
                            for base in contract.base.iter() {
                                let base_doc = base.as_doc()?;
                                let base_ident = &base.name.identifiers.last().unwrap().name;
                                bases.push(
                                    linked
                                        .as_ref()
                                        .and_then(|l| {
                                            l.get(base_ident).map(|path| {
                                                Markdown::Link(
                                                    &base_doc,
                                                    &path.display().to_string(),
                                                )
                                                .as_doc()
                                            })
                                        })
                                        .transpose()?
                                        .unwrap_or(base_doc),
                                )
                            }

                            writer.writeln_raw(bases.join(", "))?;
                            writer.writeln()?;
                        }

                        writer.writeln_doc(&item.comments)?;

                        if let Some(state_vars) = item.variables() {
                            writer.write_subtitle("State Variables")?;
                            state_vars.into_iter().try_for_each(|(item, comments, code)| {
                                let comments = comments.merge_inheritdoc(
                                    &item.name.safe_unwrap().name,
                                    read_context!(self, INHERITDOC_ID, Inheritdoc),
                                );

                                writer.write_heading(&item.name.safe_unwrap().name)?;
                                writer.write_section(&comments, code)?;
                                writer.writeln()
                            })?;
                        }

                        if let Some(funcs) = item.functions() {
                            writer.write_subtitle("Functions")?;
                            funcs.into_iter().try_for_each(|(func, comments, code)| {
                                let func_name = func
                                    .name
                                    .as_ref()
                                    .map_or(func.ty.to_string(), |n| n.name.to_owned());
                                let comments = comments.merge_inheritdoc(
                                    &func_name,
                                    read_context!(self, INHERITDOC_ID, Inheritdoc),
                                );

                                // Write function name
                                writer.write_heading(&func_name)?;
                                writer.writeln()?;

                                writer.try_write_function_section(func, &comments, code)
                            })?;
                        }

                        if let Some(events) = item.events() {
                            writer.write_subtitle("Events")?;
                            events.into_iter().try_for_each(|(event, comments, code)| {
                                writer.write_heading(&event.name.safe_unwrap().name)?;
                                writer.try_write_event_section(event, comments, code)
                            })?;
                        }

                        if let Some(errors) = item.errors() {
                            writer.write_subtitle("Errors")?;
                            errors.into_iter().try_for_each(|(error, comments, code)| {
                                writer.write_heading(&error.name.safe_unwrap().name)?;
                                writer.try_write_error_section(error, comments, code)
                            })?;
                        }

                        if let Some(structs) = item.structs() {
                            writer.write_subtitle("Structs")?;
                            structs.into_iter().try_for_each(
                                |(struct_definition, comments, code)| {
                                    writer.write_heading(
                                        &struct_definition.name.safe_unwrap().name,
                                    )?;
                                    writer.try_write_struct_section(
                                        struct_definition,
                                        comments,
                                        code,
                                    )
                                },
                            )?;
                        }

                        if let Some(enums) = item.enums() {
                            writer.write_subtitle("Enums")?;
                            enums.into_iter().try_for_each(|(item, comments, code)| {
                                writer.write_heading(&item.name.safe_unwrap().name)?;
                                writer.write_section(comments, code)
                            })?;
                        }
                    }

                    ParseSource::Function(func) => {
                        writer.try_write_function_section(func, &item.comments, &item.code)?;
                    }

                    ParseSource::Event(event) => {
                        writer.try_write_event_section(event, &item.comments, &item.code)?;
                    }

                    ParseSource::Error(error) => {
                        writer.try_write_error_section(error, &item.comments, &item.code)?;
                    }

                    ParseSource::Struct(struct_definition) => {
                        writer.try_write_struct_section(
                            struct_definition,
                            &item.comments,
                            &item.code,
                        )?;
                    }

                    ParseSource::Variable(_) | ParseSource::Enum(_) | ParseSource::Type(_) => {
                        writer.write_section(&item.comments, &item.code)?;
                    }
                }
            }
            DocumentContent::Empty => (),
        };

        Ok(writer.finish())
    }
}
