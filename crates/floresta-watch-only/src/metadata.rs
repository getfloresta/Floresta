// SPDX-License-Identifier: MIT OR Apache-2.0
#![deny(clippy::unwrap_used)]

use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;
use std::collections::HashSet;
use std::error::Error;

#[derive(Debug, Clone, Default)]
pub struct WalletMetadata {
    pub(super) name: String,
    active: ActiveDescriptorsMetadata,
    descriptors: Vec<DescriptorInfoMetadata>,
}

#[derive(Debug, Clone, Default)]
pub struct ActiveDescriptorsMetadata {
    external: Option<DescriptorInfoMetadata>,
    internal: Option<DescriptorInfoMetadata>,
}

#[derive(Debug, Clone, Default)]
pub struct DescriptorInfoMetadata {
    pub(super) id: String,
    pub(super) label: Option<String>,
    pub(super) descriptor: String,
}

#[derive(Debug)]
pub enum WalletMetadataError {
    DescriptorNotFound(String),
    DescriptorLabelConflict(String),

    ActiveDescriptorExternalNotFound,
    ActiveDescriptorInternalNotFound,
}

impl Display for WalletMetadataError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WalletMetadataError::DescriptorNotFound(id) => {
                write!(f, "Descriptor not found: {}", id)
            }
            WalletMetadataError::DescriptorLabelConflict(label) => {
                write!(f, "Descriptor label conflict: {}", label)
            }
            WalletMetadataError::ActiveDescriptorExternalNotFound => {
                write!(f, "Active external descriptor not found")
            }
            WalletMetadataError::ActiveDescriptorInternalNotFound => {
                write!(f, "Active internal descriptor not found")
            }
        }
    }
}

impl Error for WalletMetadataError {}

impl WalletMetadata {
    pub fn new(
        name: &str,
        active_external: Option<DescriptorInfoMetadata>,
        active_internal: Option<DescriptorInfoMetadata>,
        descriptors: Vec<DescriptorInfoMetadata>,
    ) -> Self {
        Self {
            name: name.to_string(),
            active: ActiveDescriptorsMetadata {
                external: active_external,
                internal: active_internal,
            },
            descriptors,
        }
    }

    pub fn get_active_descriptors(
        &self,
    ) -> Result<(DescriptorInfoMetadata, DescriptorInfoMetadata), WalletMetadataError> {
        let external = self
            .active
            .external
            .as_ref()
            .cloned()
            .ok_or(WalletMetadataError::ActiveDescriptorExternalNotFound)?;

        let internal = self
            .active
            .internal
            .as_ref()
            .cloned()
            .ok_or(WalletMetadataError::ActiveDescriptorInternalNotFound)?;

        Ok((external, internal))
    }

    pub fn get_active_descriptor(
        &self,
        is_change: bool,
    ) -> Result<DescriptorInfoMetadata, WalletMetadataError> {
        let (main, change) = self.get_active_descriptors()?;

        if is_change {
            Ok(change)
        } else {
            Ok(main)
        }
    }

    pub fn add_descriptor(
        &mut self,
        descriptor_info: DescriptorInfoMetadata,
        is_change: bool,
        is_active: bool,
    ) -> Result<Option<DescriptorInfoMetadata>, WalletMetadataError> {
        if let Some(label) = &descriptor_info.label {
            if let Some(id) = self.get_id_by_label(label) {
                if id != descriptor_info.id {
                    return Err(WalletMetadataError::DescriptorLabelConflict(label.clone()));
                }
            }
        }

        if let Err(e) = self.remover_descriptor(&descriptor_info.id) {
            if !matches!(e, WalletMetadataError::DescriptorNotFound(_)) {
                return Err(e);
            }
        }

        if is_active {
            let remove_desc = if is_change {
                self.active.internal.replace(descriptor_info)
            } else {
                self.active.external.replace(descriptor_info)
            };

            if let Some(desc) = remove_desc {
                self.descriptors.push(desc.clone());
                return Ok(Some(desc));
            }
        } else {
            self.descriptors.push(descriptor_info);
        }

        Ok(None)
    }

    pub fn remover_descriptor(
        &mut self,
        id: &str,
    ) -> Result<DescriptorInfoMetadata, WalletMetadataError> {
        if let Some(desc) = self.active.external.take() {
            if desc.id == id {
                return Ok(desc);
            }
            self.active.external = Some(desc);
        }

        if let Some(desc) = self.active.internal.take() {
            if desc.id == id {
                return Ok(desc);
            }
            self.active.internal = Some(desc);
        }

        self.descriptors
            .iter()
            .position(|d| d.id == id)
            .map(|index| self.descriptors.remove(index))
            .ok_or_else(|| WalletMetadataError::DescriptorNotFound(id.to_string()))
    }

    pub fn get_ids(&self) -> HashSet<String> {
        let capacity = 2 + self.descriptors.len();
        let mut ids = HashSet::with_capacity(capacity);

        ids.extend(self.descriptors.iter().map(|d| d.id.clone()));

        if let Some(default_descriptor) = &self.active.external {
            ids.insert(default_descriptor.id.clone());
        }

        if let Some(change_desc) = &self.active.internal {
            ids.insert(change_desc.id.clone());
        }

        ids
    }

    pub fn get_descriptors(&self) -> Vec<&DescriptorInfoMetadata> {
        let all_descriptors_capacity = 2 + self.descriptors.len();
        let mut all_descriptors = Vec::with_capacity(all_descriptors_capacity);

        all_descriptors.extend(self.descriptors.iter());

        if let Some(default_descriptor) = &self.active.external {
            all_descriptors.push(default_descriptor);
        }

        if let Some(change_desc) = &self.active.internal {
            all_descriptors.push(change_desc);
        }

        all_descriptors
    }

    pub fn get_id_by_label(&self, label: &str) -> Option<String> {
        self.active
            .external
            .as_ref()
            .filter(|d| d.label.as_deref() == Some(label))
            .or_else(|| {
                self.active
                    .internal
                    .as_ref()
                    .filter(|d| d.label.as_deref() == Some(label))
            })
            .or_else(|| {
                self.descriptors
                    .iter()
                    .find(|d| d.label.as_deref() == Some(label))
            })
            .map(|d| d.id.clone())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    use super::*;

    fn create_descriptor(id: &str, label: Option<&str>) -> DescriptorInfoMetadata {
        DescriptorInfoMetadata {
            id: id.to_string(),
            descriptor: format!("descriptor_{}", id),
            label: label.map(|l| l.to_string()),
        }
    }

    #[test]
    fn test_wallet_metadata_creation() {
        let external = create_descriptor("external", Some("Receiving"));
        let internal = create_descriptor("internal", Some("Change"));
        let descriptors = vec![create_descriptor("desc1", None)];

        let wallet = WalletMetadata::new(
            "test_wallet",
            Some(external.clone()),
            Some(internal.clone()),
            descriptors,
        );

        assert_eq!(wallet.name, "test_wallet");
        assert_eq!(
            wallet.active.external.as_ref().map(|d| d.id.as_str()),
            Some("external")
        );
        assert_eq!(
            wallet.active.internal.as_ref().map(|d| d.id.as_str()),
            Some("internal")
        );
    }

    #[test]
    fn test_wallet_metadata_creation_without_active_descriptors() {
        let descriptors = vec![
            create_descriptor("desc1", Some("Descriptor 1")),
            create_descriptor("desc2", None),
        ];

        let wallet = WalletMetadata::new("wallet_no_active", None, None, descriptors);

        assert_eq!(wallet.name, "wallet_no_active");
        assert!(wallet.active.external.is_none());
        assert!(wallet.active.internal.is_none());
    }

    #[test]
    fn test_get_active_descriptors_success() {
        let external = create_descriptor("external", Some("Receiving"));
        let internal = create_descriptor("internal", Some("Change"));

        let wallet = WalletMetadata::new(
            "wallet",
            Some(external.clone()),
            Some(internal.clone()),
            vec![],
        );

        let result = wallet.get_active_descriptors();
        assert!(result.is_ok());

        let (ext, int) = result.unwrap();
        assert_eq!(ext.id, "external");
        assert_eq!(int.id, "internal");
    }

    #[test]
    fn test_get_active_descriptors_missing_external() {
        let internal = create_descriptor("internal", Some("Change"));
        let wallet = WalletMetadata::new("wallet", None, Some(internal), vec![]);

        let result = wallet.get_active_descriptors();
        assert!(matches!(
            result,
            Err(WalletMetadataError::ActiveDescriptorExternalNotFound)
        ));
    }

    #[test]
    fn test_get_active_descriptors_missing_internal() {
        let external = create_descriptor("external", Some("Receiving"));
        let wallet = WalletMetadata::new("wallet", Some(external), None, vec![]);

        let result = wallet.get_active_descriptors();
        assert!(matches!(
            result,
            Err(WalletMetadataError::ActiveDescriptorInternalNotFound)
        ));
    }

    #[test]
    fn test_get_active_descriptor_external() {
        let external = create_descriptor("external", Some("Receiving"));
        let internal = create_descriptor("internal", Some("Change"));

        let wallet = WalletMetadata::new("wallet", Some(external.clone()), Some(internal), vec![]);

        let result = wallet.get_active_descriptor(false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "external");
    }

    #[test]
    fn test_get_active_descriptor_internal() {
        let external = create_descriptor("external", Some("Receiving"));
        let internal = create_descriptor("internal", Some("Change"));

        let wallet = WalletMetadata::new("wallet", Some(external), Some(internal.clone()), vec![]);

        let result = wallet.get_active_descriptor(true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "internal");
    }

    #[test]
    fn test_get_active_descriptor_missing() {
        let wallet = WalletMetadata::new("wallet", None, None, vec![]);

        let result = wallet.get_active_descriptor(false);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_descriptor_to_empty_wallet() {
        let mut wallet = WalletMetadata::new("wallet", None, None, vec![]);
        let descriptor = create_descriptor("desc1", Some("First"));

        let result = wallet.add_descriptor(descriptor.clone(), false, false);
        assert!(result.is_ok());
        assert_eq!(wallet.descriptors.len(), 1);
        assert_eq!(wallet.descriptors[0].id, "desc1");
    }

    #[test]
    fn test_add_descriptor_as_active_external() {
        let mut wallet = WalletMetadata::new("wallet", None, None, vec![]);
        let descriptor = create_descriptor("external", Some("Receiving"));

        let result = wallet.add_descriptor(descriptor, false, true);
        assert!(result.is_ok());
        assert!(wallet.active.external.is_some());
        assert_eq!(wallet.active.external.as_ref().unwrap().id, "external");
    }

    #[test]
    fn test_add_descriptor_as_active_internal() {
        let mut wallet = WalletMetadata::new("wallet", None, None, vec![]);
        let descriptor = create_descriptor("internal", Some("Change"));

        let result = wallet.add_descriptor(descriptor, true, true);
        assert!(result.is_ok());
        assert!(wallet.active.internal.is_some());
        assert_eq!(wallet.active.internal.as_ref().unwrap().id, "internal");
    }

    #[test]
    fn test_add_descriptor_replaces_existing_active() {
        let old_external = create_descriptor("old_external", Some("Old Receiving"));
        let mut wallet = WalletMetadata::new("wallet", Some(old_external), None, vec![]);

        let new_external = create_descriptor("new_external", Some("New Receiving"));
        let result = wallet.add_descriptor(new_external, false, true);

        assert!(result.is_ok());
        assert_eq!(wallet.active.external.as_ref().unwrap().id, "new_external");
        // The old external should now be in the descriptors list
        assert_eq!(wallet.descriptors.len(), 1);
        assert_eq!(wallet.descriptors[0].id, "old_external");
    }

    #[test]
    fn test_remove_descriptor_from_inactive() {
        let descriptor = create_descriptor("desc1", Some("Descriptor 1"));
        let mut wallet = WalletMetadata::new("wallet", None, None, vec![descriptor.clone()]);

        let result = wallet.remover_descriptor("desc1");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "desc1");
        assert!(wallet.descriptors.is_empty());
    }

    #[test]
    fn test_remove_active_external_descriptor() {
        let external = create_descriptor("external", Some("Receiving"));
        let mut wallet = WalletMetadata::new("wallet", Some(external), None, vec![]);

        let result = wallet.remover_descriptor("external");
        assert!(result.is_ok());
        assert!(wallet.active.external.is_none());
    }

    #[test]
    fn test_remove_active_internal_descriptor() {
        let internal = create_descriptor("internal", Some("Change"));
        let mut wallet = WalletMetadata::new("wallet", None, Some(internal), vec![]);

        let result = wallet.remover_descriptor("internal");
        assert!(result.is_ok());
        assert!(wallet.active.internal.is_none());
    }

    #[test]
    fn test_remove_nonexistent_descriptor() {
        let mut wallet = WalletMetadata::new("wallet", None, None, vec![]);

        let result = wallet.remover_descriptor("nonexistent");
        assert!(matches!(
            result,
            Err(WalletMetadataError::DescriptorNotFound(ref id)) if id == "nonexistent"
        ));
    }

    #[test]
    fn test_get_ids_with_all_descriptors() {
        let external = create_descriptor("external", None);
        let internal = create_descriptor("internal", None);
        let descriptors = vec![
            create_descriptor("desc1", None),
            create_descriptor("desc2", None),
        ];

        let wallet = WalletMetadata::new("wallet", Some(external), Some(internal), descriptors);

        let ids = wallet.get_ids();
        assert_eq!(ids.len(), 4);
        assert!(ids.contains("external"));
        assert!(ids.contains("internal"));
        assert!(ids.contains("desc1"));
        assert!(ids.contains("desc2"));
    }

    #[test]
    fn test_get_ids_only_active() {
        let external = create_descriptor("external", None);
        let internal = create_descriptor("internal", None);

        let wallet = WalletMetadata::new("wallet", Some(external), Some(internal), vec![]);

        let ids = wallet.get_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("external"));
        assert!(ids.contains("internal"));
    }

    #[test]
    fn test_get_ids_with_duplicates_prevention() {
        // Se um descriptor está nos ativos E na lista, deve aparecer apenas uma vez
        let external = create_descriptor("external", None);
        let descriptors = vec![create_descriptor("external", None)]; // Mesmo ID na lista

        let wallet = WalletMetadata::new("wallet", Some(external), None, descriptors);

        let ids = wallet.get_ids();
        assert_eq!(ids.len(), 1);
        assert!(ids.contains("external"));
    }

    #[test]
    fn test_get_descriptors_all_types() {
        let external = create_descriptor("external", Some("Receiving"));
        let internal = create_descriptor("internal", Some("Change"));
        let descriptors = vec![
            create_descriptor("desc1", Some("Extra 1")),
            create_descriptor("desc2", Some("Extra 2")),
        ];

        let wallet = WalletMetadata::new("wallet", Some(external), Some(internal), descriptors);

        let all = wallet.get_descriptors();
        assert_eq!(all.len(), 4);
        assert!(all.iter().any(|d| d.id == "external"));
        assert!(all.iter().any(|d| d.id == "internal"));
        assert!(all.iter().any(|d| d.id == "desc1"));
        assert!(all.iter().any(|d| d.id == "desc2"));
    }

    #[test]
    fn test_get_descriptors_empty_wallet() {
        let wallet = WalletMetadata::new("wallet", None, None, vec![]);

        let all = wallet.get_descriptors();
        assert!(all.is_empty());
    }

    #[test]
    fn test_get_id_by_label_from_external() {
        let external = create_descriptor("external", Some("Receiving Address"));
        let wallet = WalletMetadata::new("wallet", Some(external), None, vec![]);

        let result = wallet.get_id_by_label("Receiving Address");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "external");
    }

    #[test]
    fn test_get_id_by_label_from_internal() {
        let internal = create_descriptor("internal", Some("Change Address"));
        let wallet = WalletMetadata::new("wallet", None, Some(internal), vec![]);

        let result = wallet.get_id_by_label("Change Address");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "internal");
    }

    #[test]
    fn test_get_id_by_label_from_list() {
        let descriptors = vec![create_descriptor("desc1", Some("My Label"))];
        let wallet = WalletMetadata::new("wallet", None, None, descriptors);

        let result = wallet.get_id_by_label("My Label");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "desc1");
    }

    #[test]
    fn test_get_id_by_label_not_found() {
        let wallet = WalletMetadata::new("wallet", None, None, vec![]);

        let result = wallet.get_id_by_label("Nonexistent Label");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_id_by_label_returns_first_match() {
        // If there are multiple descriptors with the same label, it should return the first one found (external > internal > list)
        let external = create_descriptor("external", Some("Shared Label"));
        let descriptors = vec![create_descriptor("desc1", Some("Shared Label"))];
        let wallet = WalletMetadata::new("wallet", Some(external), None, descriptors);

        let result = wallet.get_id_by_label("Shared Label");
        assert!(result.is_some()); // Pode ser "external" ou "desc1"
        assert!(["external", "desc1"].contains(&result.unwrap().as_str()));
    }

    #[test]
    fn test_descriptor_info_metadata_creation() {
        let desc = DescriptorInfoMetadata {
            id: "test_id".to_string(),
            descriptor: "wpkh(...)".to_string(),
            label: Some("My Descriptor".to_string()),
        };

        assert_eq!(desc.id, "test_id");
        assert_eq!(desc.descriptor, "wpkh(...)");
        assert_eq!(desc.label, Some("My Descriptor".to_string()));
    }

    #[test]
    fn test_descriptor_info_metadata_without_label() {
        let desc = DescriptorInfoMetadata {
            id: "test_id".to_string(),
            descriptor: "wpkh(...)".to_string(),
            label: None,
        };

        assert_eq!(desc.id, "test_id");
        assert_eq!(desc.descriptor, "wpkh(...)");
        assert!(desc.label.is_none());
    }

    #[test]
    fn test_complex_workflow() {
        let mut wallet = WalletMetadata::new("my_wallet", None, None, vec![]);

        // Add active external descriptor
        let external = create_descriptor("ext1", Some("Main Receiving"));
        wallet.add_descriptor(external, false, true).unwrap();
        assert_eq!(wallet.get_ids().len(), 1);

        // Add active internal descriptor
        let internal = create_descriptor("int1", Some("Change"));
        wallet.add_descriptor(internal, true, true).unwrap();
        assert_eq!(wallet.get_ids().len(), 2);

        // Add inactive descriptors
        let desc2 = create_descriptor("desc2", Some("Extra 1"));
        wallet.add_descriptor(desc2, false, false).unwrap();

        let desc3 = create_descriptor("desc3", Some("Extra 2"));
        wallet.add_descriptor(desc3, false, false).unwrap();

        // Check state
        assert_eq!(wallet.get_ids().len(), 4);
        assert_eq!(wallet.get_descriptors().len(), 4);

        // Replace active external descriptor
        let new_external = create_descriptor("ext2", Some("New Receiving"));
        wallet.add_descriptor(new_external, false, true).unwrap();
        assert_eq!(wallet.get_ids().len(), 5); // ext1 should now be in the list

        // Remove a descriptor
        wallet.remover_descriptor("desc2").unwrap();
        assert_eq!(wallet.get_ids().len(), 4);

        // Check that we can retrieve by label
        assert_eq!(wallet.get_id_by_label("Change"), Some("int1".to_string()));
    }
}
