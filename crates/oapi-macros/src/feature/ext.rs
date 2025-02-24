use proc_macro2::TokenStream;

use crate::feature::Feature;
use crate::feature::attributes::{Rename, RenameAll, Style, ValueType};
use crate::type_tree::TypeTree;
use crate::{DiagResult, TryToTokens};

use super::attributes::ParameterIn;

pub(crate) trait TryToTokensExt {
    fn try_to_token_stream(&self) -> DiagResult<TokenStream>;
}

impl TryToTokensExt for Vec<Feature> {
    fn try_to_token_stream(&self) -> DiagResult<TokenStream> {
        let mut tokens = TokenStream::new();
        for item in self.iter() {
            item.try_to_tokens(&mut tokens)?;
        }
        Ok(tokens)
    }
}

pub(crate) trait FeaturesExt {
    fn pop_by(&mut self, op: impl FnMut(&Feature) -> bool) -> Option<Feature>;

    fn pop_value_type_feature(&mut self) -> Option<ValueType>;

    /// Pop [`parameter_in`] feature if exists in [`Vec<Feature>`] list.
    fn pop_parameter_in_feature(&mut self) -> Option<ParameterIn>;

    /// Pop [`style`] feature if exists in [`Vec<Feature>`] list.
    fn pop_style_feature(&mut self) -> Option<Style>;

    /// Pop [`Rename`] feature if exists in [`Vec<Feature>`] list.
    fn pop_rename_feature(&mut self) -> Option<Rename>;

    /// Pop [`RenameAll`] feature if exists in [`Vec<Feature>`] list.
    fn pop_rename_all_feature(&mut self) -> Option<RenameAll>;

    /// Extract [`XmlAttr`] feature for given `type_tree` if it has generic type [`GenericType::Vec`]
    fn extract_vec_xml_feature(&mut self, type_tree: &TypeTree) -> Option<Feature>;
}

impl FeaturesExt for Vec<Feature> {
    fn pop_by(&mut self, op: impl FnMut(&Feature) -> bool) -> Option<Feature> {
        self.iter()
            .position(op)
            .map(|index| self.swap_remove(index))
    }

    fn pop_value_type_feature(&mut self) -> Option<ValueType> {
        self.pop_by(|feature| matches!(feature, Feature::ValueType(_)))
            .and_then(|feature| match feature {
                Feature::ValueType(value_type) => Some(value_type),
                _ => None,
            })
    }

    fn pop_parameter_in_feature(&mut self) -> Option<ParameterIn> {
        self.pop_by(|feature| matches!(feature, Feature::ParameterIn(_)))
            .and_then(|feature| match feature {
                Feature::ParameterIn(parameter_in) => Some(parameter_in),
                _ => None,
            })
    }

    fn pop_style_feature(&mut self) -> Option<Style> {
        self.pop_by(|feature| matches!(feature, Feature::Style(_)))
            .and_then(|feature| match feature {
                Feature::Style(style) => Some(style),
                _ => None,
            })
    }

    fn pop_rename_feature(&mut self) -> Option<Rename> {
        self.pop_by(|feature| matches!(feature, Feature::Rename(_)))
            .and_then(|feature| match feature {
                Feature::Rename(rename) => Some(rename),
                _ => None,
            })
    }

    fn pop_rename_all_feature(&mut self) -> Option<RenameAll> {
        self.pop_by(|feature| matches!(feature, Feature::RenameAll(_)))
            .and_then(|feature| match feature {
                Feature::RenameAll(rename_all) => Some(rename_all),
                _ => None,
            })
    }

    fn extract_vec_xml_feature(&mut self, type_tree: &TypeTree) -> Option<Feature> {
        self.iter_mut().find_map(|feature| match feature {
            Feature::XmlAttr(xml_feature) => {
                let Ok((vec_xml, value_xml)) = xml_feature.split_for_vec(type_tree) else {
                    return None;
                };

                // replace the original xml attribute with split value xml
                if let Some(mut xml) = value_xml {
                    std::mem::swap(xml_feature, &mut xml)
                }

                vec_xml.map(Feature::XmlAttr)
            }
            _ => None,
        })
    }
}

impl FeaturesExt for Option<Vec<Feature>> {
    fn pop_by(&mut self, op: impl FnMut(&Feature) -> bool) -> Option<Feature> {
        self.as_mut().and_then(|features| features.pop_by(op))
    }

    fn pop_value_type_feature(&mut self) -> Option<ValueType> {
        self.as_mut()
            .and_then(|features| features.pop_value_type_feature())
    }

    fn pop_parameter_in_feature(&mut self) -> Option<ParameterIn> {
        self.as_mut()
            .and_then(|features| features.pop_parameter_in_feature())
    }

    fn pop_style_feature(&mut self) -> Option<Style> {
        self.as_mut()
            .and_then(|features| features.pop_style_feature())
    }

    fn pop_rename_feature(&mut self) -> Option<Rename> {
        self.as_mut()
            .and_then(|features| features.pop_rename_feature())
    }

    fn pop_rename_all_feature(&mut self) -> Option<RenameAll> {
        self.as_mut()
            .and_then(|features| features.pop_rename_all_feature())
    }

    fn extract_vec_xml_feature(&mut self, type_tree: &TypeTree) -> Option<Feature> {
        self.as_mut()
            .and_then(|features| features.extract_vec_xml_feature(type_tree))
    }
}
