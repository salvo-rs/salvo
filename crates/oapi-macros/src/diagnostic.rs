use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};

/// Diagnostic levels used by the macro implementation.
///
/// All diagnostics currently produced by this crate are errors. Help and note
/// messages are attached through [`Diagnostic::help`] and [`Diagnostic::note`].
#[derive(Clone, Copy, Debug)]
pub(crate) enum Level {
    Error,
}

#[derive(Debug)]
pub(crate) struct Diagnostic {
    inner: Inner,
}

#[derive(Debug)]
enum Inner {
    Syn(syn::Error),
    Message {
        span: Span,
        message: String,
        children: Vec<Child>,
    },
}

#[derive(Debug)]
struct Child {
    level: &'static str,
    message: String,
}

impl Diagnostic {
    pub(crate) fn new(message_level: Level, message: impl Into<String>) -> Self {
        Self::spanned(Span::call_site(), message_level, message)
    }

    pub(crate) fn spanned(span: Span, _level: Level, message: impl Into<String>) -> Self {
        Self {
            inner: Inner::Message {
                span,
                message: message.into(),
                children: Vec::new(),
            },
        }
    }

    pub(crate) fn help(self, message: impl Into<String>) -> Self {
        self.child("help", message)
    }

    pub(crate) fn note(self, message: impl Into<String>) -> Self {
        self.child("note", message)
    }

    fn child(mut self, level: &'static str, message: impl Into<String>) -> Self {
        let message = message.into();
        match &mut self.inner {
            Inner::Message { children, .. } => children.push(Child { level, message }),
            Inner::Syn(error) => {
                error.combine(syn::Error::new(error.span(), format!("{level}: {message}")));
            }
        }
        self
    }

    pub(crate) fn emit_as_item_tokens(self) -> TokenStream {
        let error: syn::Error = self.into();
        error.to_compile_error()
    }

    pub(crate) fn emit_as_expr_tokens(self) -> TokenStream {
        let error: syn::Error = self.into();
        let compile_errors = error.into_iter().map(|error| {
            let span = error.span();
            let compile_error = error.to_compile_error();
            quote_spanned!(span => #compile_error;)
        });
        quote!({ #(#compile_errors)* })
    }
}

impl From<syn::Error> for Diagnostic {
    fn from(error: syn::Error) -> Self {
        Self {
            inner: Inner::Syn(error),
        }
    }
}

impl From<Diagnostic> for syn::Error {
    fn from(diagnostic: Diagnostic) -> Self {
        match diagnostic.inner {
            Inner::Syn(error) => error,
            Inner::Message {
                span,
                mut message,
                children,
            } => {
                for child in children {
                    message.push('\n');
                    message.push_str(child.level);
                    message.push_str(": ");
                    message.push_str(&child.message);
                }
                syn::Error::new(span, message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, Level};

    #[test]
    fn preserves_help_and_note_messages() {
        let diagnostic = Diagnostic::new(Level::Error, "invalid attribute")
            .help("remove the attribute")
            .note("only supported on structs");
        let error: syn::Error = diagnostic.into();

        assert_eq!(
            error.to_string(),
            "invalid attribute\nhelp: remove the attribute\nnote: only supported on structs"
        );
    }

    #[test]
    fn preserves_syn_errors() {
        let diagnostic = Diagnostic::from(syn::Error::new(
            proc_macro2::Span::call_site(),
            "parse failed",
        ));
        let error: syn::Error = diagnostic.into();

        assert_eq!(error.to_string(), "parse failed");
    }
}
