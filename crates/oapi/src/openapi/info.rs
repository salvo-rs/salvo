//! Implements [OpenAPI Metadata][info] types.
//!
//! Refer to [`OpenApi`][openapi_trait] trait and [derive documentation][derive]
//! for examples and usage details.
//!
//! [info]: <https://spec.openapis.org/oas/latest.html#info-object>
//! [openapi_trait]: ../../trait.OpenApi.html
//! [derive]: ../../derive.OpenApi.html
use serde::{Deserialize, Serialize};

/// # Examples
///
/// Create [`Info`]].
/// ```
/// # use salvo_oapi::{Info, Contact};
/// let info = Info::new("My api", "1.0.0").contact(Contact::new()
///     .name("Admin Admin")
///     .email("amdin@petapi.com")
/// );
/// ```
/// OpenAPI [Info][info] object represents metadata of the API.
///
/// You can use [`Info::new`] to construct a new [`Info`] object.
///
/// [info]: <https://spec.openapis.org/oas/latest.html#info-object>
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    /// Title of the API.
    pub title: String,

    /// Optional description of the API.
    ///
    /// Value supports markdown syntax.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional url for terms of service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms_of_service: Option<String>,

    /// Contact information of exposed API.
    ///
    /// See more details at: <https://spec.openapis.org/oas/latest.html#contact-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<Contact>,

    /// License of the API.
    ///
    /// See more details at: <https://spec.openapis.org/oas/latest.html#license-object>.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,

    /// Document version typically the API version.
    pub version: String,
}

impl Info {
    /// Construct a new [`Info`] object.
    ///
    /// This function accepts two arguments. One which is the title of the API and two the
    /// version of the api document typically the API version.
    ///
    /// # Examples
    ///
    /// ```
    /// # use salvo_oapi::Info;
    /// let info = Info::new("Pet api", "1.1.0");
    /// ```
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            version: version.into(),
            ..Default::default()
        }
    }
    /// Add title of the API.
    pub fn title<I: Into<String>>(mut self, title: I) -> Self {
        self.title = title.into();
        self
    }

    /// Add version of the api document typically the API version.
    pub fn version<I: Into<String>>(mut self, version: I) -> Self {
        self.version = version.into();
        self
    }

    /// Add description of the API.
    pub fn description<S: Into<String>>(mut self, description: S) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add url for terms of the API.
    pub fn terms_of_service<S: Into<String>>(mut self, terms_of_service: S) -> Self {
        self.terms_of_service = Some(terms_of_service.into());
        self
    }

    /// Add contact information of the API.
    pub fn contact(mut self, contact: Contact) -> Self {
        self.contact = Some(contact);
        self
    }

    /// Add license of the API.
    pub fn license(mut self, license: License) -> Self {
        self.license = Some(license);
        self
    }
}

/// OpenAPI [Contact][contact] information of the API.
///
/// You can use [`Contact::new`] to construct a new [`Contact`] object.
///
/// [contact]: <https://spec.openapis.org/oas/latest.html#contact-object>
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    /// Identifying name of the contact person or organization of the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Url pointing to contact information of the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Email of the contact person or the organization of the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

impl Contact {
    /// Construct a new empty [`Contact`]. This is effectively same as calling [`Contact::default`].
    pub fn new() -> Self {
        Default::default()
    }
    /// Add name contact person or organization of the API.
    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add url pointing to the contact information of the API.
    pub fn url<S: Into<String>>(mut self, url: S) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Add email of the contact person or organization of the API.
    pub fn email<S: Into<String>>(mut self, email: S) -> Self {
        self.email = Some(email.into());
        self
    }
}

/// OpenAPI [License][license] information of the API.
///
/// [license]: <https://spec.openapis.org/oas/latest.html#license-object>
#[non_exhaustive]
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct License {
    /// Name of the license used e.g MIT or Apache-2.0
    pub name: String,

    /// Optional url pointing to the license.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl License {
    /// Construct a new [`License`] object.
    ///
    /// Function takes name of the license as an argument e.g MIT.
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }
    /// Add name of the license used in API.
    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = name.into();
        self
    }

    /// Add url pointing to the license used in API.
    pub fn url<S: Into<String>>(mut self, url: S) -> Self {
        self.url = Some(url.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Contact;

    #[test]
    fn contact_new() {
        let contact = Contact::new();

        assert!(contact.name.is_none());
        assert!(contact.url.is_none());
        assert!(contact.email.is_none());
    }
}
