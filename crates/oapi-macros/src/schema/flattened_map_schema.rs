use crate::feature::{pop_feature, Feature, FeaturesExt};
use crate::{ComponentSchema, ComponentSchemaProps};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

#[derive(Debug)]
pub(crate) struct FlattenedMapSchema {
    tokens: TokenStream,
}

impl FlattenedMapSchema {
    pub(crate) fn new(
        ComponentSchemaProps {
            type_tree,
            features,
            description,
            deprecated,
            object_name,
            type_definition,
        }: ComponentSchemaProps,
    ) -> Self {
        let mut tokens = TokenStream::new();
        let mut features = features.unwrap_or(Vec::new());
        let deprecated_stream = ComponentSchema::get_deprecated(deprecated);
        let description_stream = ComponentSchema::get_description(description);

        let example = features.pop_by(|feature| matches!(feature, Feature::Example(_)));
        let nullable = pop_feature!(features => Feature::Nullable(_));
        let default = pop_feature!(features => Feature::Default(_));

        // Maps are treated as generic objects with no named properties and
        // additionalProperties denoting the type
        // maps have 2 child schemas and we are interested the second one of them
        // which is used to determine the additional properties
        let schema_property = ComponentSchema::new(ComponentSchemaProps {
            type_tree: type_tree
                .children
                .as_ref()
                .expect("`ComponentSchema` Map type should have children")
                .iter()
                .nth(1)
                .expect("`ComponentSchema` Map type should have 2 child"),
            features: Some(features),
            description: None,
            deprecated: None,
            object_name,
            type_definition,
        });

        tokens.extend(quote! {
            #schema_property
                #description_stream
                #deprecated_stream
                #default
        });

        example.to_tokens(&mut tokens);
        nullable.to_tokens(&mut tokens);

        Self { tokens }
    }
}

impl ToTokens for FlattenedMapSchema {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens)
    }
}
