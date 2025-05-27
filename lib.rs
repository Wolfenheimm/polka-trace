#![cfg_attr(not(feature = "std"), no_std, no_main)]

#[ink::contract]
mod polka_trace {
    use ink::prelude::vec::Vec;
    use ink::storage::Mapping;

    /// Represents different types of lifecycle events  
    #[derive(
        Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode,
    )]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum EventType {
        Created,
        Shipped,
        InTransit,
        Received,
        Inspected,
        Verified,
        Delivered,
    }

    /// Custom errors for the contract
    #[derive(Debug, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum PolkaTraceError {
        ProductAlreadyExists,
        UnauthorizedAccess,
        ProductNotFound,
        InvalidEvent,
    }

    /// Result type for contract operations
    pub type Result<T> = core::result::Result<T, PolkaTraceError>;

    /// Main contract storage
    #[ink(storage)]
    pub struct PolkaTrace {
        /// Maps product ID to product owner
        product_owners: Mapping<u128, AccountId>,
        /// Maps product ID to manufacturer
        product_manufacturers: Mapping<u128, AccountId>,
        /// Maps product ID to metadata
        product_metadata: Mapping<u128, Vec<u8>>,
        /// Maps product ID to creation timestamp
        product_created_at: Mapping<u128, Timestamp>,
        /// Maps product ID to number of events
        product_event_count: Mapping<u128, u32>,
        /// Maps owner to list of their product IDs
        owner_products: Mapping<AccountId, Vec<u128>>,
        /// Maps manufacturer to list of their product IDs
        manufacturer_products: Mapping<AccountId, Vec<u128>>,
        /// Tracks authorized accounts for logging events
        authorized_accounts: Mapping<AccountId, bool>,
        /// Contract admin
        admin: AccountId,
        /// Next product ID to prevent collisions
        next_product_id: u128,
    }

    /// Events emitted by the contract
    #[ink(event)]
    pub struct ProductRegistered {
        #[ink(topic)]
        product_id: u128,
        #[ink(topic)]
        manufacturer: AccountId,
    }

    #[ink(event)]
    pub struct LifecycleEventLogged {
        #[ink(topic)]
        product_id: u128,
        event_type: EventType,
        #[ink(topic)]
        actor: AccountId,
    }

    #[ink(event)]
    pub struct OwnershipTransferred {
        #[ink(topic)]
        product_id: u128,
        #[ink(topic)]
        from: AccountId,
        #[ink(topic)]
        to: AccountId,
    }

    impl PolkaTrace {
        /// Constructor that initializes the contract
        #[ink(constructor)]
        pub fn new() -> Self {
            let caller = Self::env().caller();
            let mut contract = Self {
                product_owners: Mapping::default(),
                product_manufacturers: Mapping::default(),
                product_metadata: Mapping::default(),
                product_created_at: Mapping::default(),
                product_event_count: Mapping::default(),
                owner_products: Mapping::default(),
                manufacturer_products: Mapping::default(),
                authorized_accounts: Mapping::default(),
                admin: caller,
                next_product_id: 1,
            };

            // Admin is automatically authorized
            contract.authorized_accounts.insert(caller, &true);
            contract
        }

        /// Register a new product in the supply chain
        #[ink(message)]
        pub fn register_product(&mut self, metadata: Vec<u8>) -> Result<u128> {
            let caller = self.env().caller();
            let product_id = self.next_product_id;
            self.next_product_id = self.next_product_id.checked_add(1).unwrap_or(u128::MAX);

            let timestamp = self.env().block_timestamp();

            // Store product data
            self.product_owners.insert(product_id, &caller);
            self.product_manufacturers.insert(product_id, &caller);
            self.product_metadata.insert(product_id, &metadata);
            self.product_created_at.insert(product_id, &timestamp);
            self.product_event_count.insert(product_id, &1); // Start with 1 (created event)

            // Add to manufacturer's product list
            let mut manufacturer_products =
                self.manufacturer_products.get(caller).unwrap_or_default();
            manufacturer_products.push(product_id);
            self.manufacturer_products
                .insert(caller, &manufacturer_products);

            // Add to owner's product list
            let mut owner_products = self.owner_products.get(caller).unwrap_or_default();
            owner_products.push(product_id);
            self.owner_products.insert(caller, &owner_products);

            // Emit event
            self.env().emit_event(ProductRegistered {
                product_id,
                manufacturer: caller,
            });

            Ok(product_id)
        }

        /// Log a new lifecycle event for a product
        #[ink(message)]
        pub fn log_event(&mut self, product_id: u128, event_type: EventType) -> Result<()> {
            let caller = self.env().caller();

            // Check if caller is authorized
            if !self.is_authorized(caller) {
                return Err(PolkaTraceError::UnauthorizedAccess);
            }

            // Check if product exists
            if !self.product_owners.contains(product_id) {
                return Err(PolkaTraceError::ProductNotFound);
            }

            // Increment event count
            let current_count = self.product_event_count.get(product_id).unwrap_or(0);
            let new_count = current_count.checked_add(1).unwrap_or(u32::MAX);
            self.product_event_count.insert(product_id, &new_count);

            // Handle ownership transfer for received events (event_type = Received)
            if event_type == EventType::Received {
                self.transfer_ownership_internal(product_id, caller)?;
            }

            // Emit event
            self.env().emit_event(LifecycleEventLogged {
                product_id,
                event_type,
                actor: caller,
            });

            Ok(())
        }

        /// Verify if a product exists and is authentic
        #[ink(message)]
        pub fn verify_product(&self, product_id: u128) -> bool {
            self.product_owners.contains(product_id)
        }

        /// Get basic product information
        #[ink(message)]
        pub fn get_product(
            &self,
            product_id: u128,
        ) -> Option<(AccountId, AccountId, Vec<u8>, Timestamp, u32)> {
            if !self.product_owners.contains(product_id) {
                return None;
            }

            let owner = self.product_owners.get(product_id)?;
            let manufacturer = self.product_manufacturers.get(product_id)?;
            let metadata = self.product_metadata.get(product_id)?;
            let created_at = self.product_created_at.get(product_id)?;
            let event_count = self.product_event_count.get(product_id).unwrap_or(0);

            Some((owner, manufacturer, metadata, created_at, event_count))
        }

        /// Get all product IDs owned by a specific account
        #[ink(message)]
        pub fn get_products_by_owner(&self, owner: AccountId) -> Vec<u128> {
            self.owner_products.get(owner).unwrap_or_default()
        }

        /// Get all product IDs manufactured by a specific account
        #[ink(message)]
        pub fn get_products_by_manufacturer(&self, manufacturer: AccountId) -> Vec<u128> {
            self.manufacturer_products
                .get(manufacturer)
                .unwrap_or_default()
        }

        /// Add an authorized account (admin only)
        #[ink(message)]
        pub fn add_authorized_account(&mut self, account: AccountId) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.admin {
                return Err(PolkaTraceError::UnauthorizedAccess);
            }

            self.authorized_accounts.insert(account, &true);
            Ok(())
        }

        /// Remove an authorized account (admin only)
        #[ink(message)]
        pub fn remove_authorized_account(&mut self, account: AccountId) -> Result<()> {
            let caller = self.env().caller();
            if caller != self.admin {
                return Err(PolkaTraceError::UnauthorizedAccess);
            }

            self.authorized_accounts.remove(account);
            Ok(())
        }

        /// Check if an account is authorized
        #[ink(message)]
        pub fn is_authorized(&self, account: AccountId) -> bool {
            self.authorized_accounts.get(account).unwrap_or(false) || account == self.admin
        }

        /// Get the contract admin
        #[ink(message)]
        pub fn get_admin(&self) -> AccountId {
            self.admin
        }

        /// Internal function to handle ownership transfer
        fn transfer_ownership_internal(
            &mut self,
            product_id: u128,
            new_owner: AccountId,
        ) -> Result<()> {
            let old_owner = self
                .product_owners
                .get(product_id)
                .ok_or(PolkaTraceError::ProductNotFound)?;

            // Update product owner
            self.product_owners.insert(product_id, &new_owner);

            // Remove from old owner's list
            let mut old_owner_products = self.owner_products.get(old_owner).unwrap_or_default();
            old_owner_products.retain(|&id| id != product_id);
            self.owner_products.insert(old_owner, &old_owner_products);

            // Add to new owner's list
            let mut new_owner_products = self.owner_products.get(new_owner).unwrap_or_default();
            new_owner_products.push(product_id);
            self.owner_products.insert(new_owner, &new_owner_products);

            // Emit ownership transfer event
            self.env().emit_event(OwnershipTransferred {
                product_id,
                from: old_owner,
                to: new_owner,
            });

            Ok(())
        }
    }

    /// Unit tests
    #[cfg(test)]
    mod tests {
        use super::*;

        // Helper function to create test accounts
        fn account(id: u8) -> AccountId {
            AccountId::from([id; 32])
        }

        // Test accounts representing different supply chain actors
        fn manufacturer() -> AccountId {
            account(1)
        }
        fn distributor() -> AccountId {
            account(2)
        }
        fn retailer() -> AccountId {
            account(3)
        }
        fn consumer() -> AccountId {
            account(4)
        }
        fn logistics_company() -> AccountId {
            account(5)
        }
        fn quality_inspector() -> AccountId {
            account(6)
        }

        #[ink::test]
        fn constructor_works() {
            let contract = PolkaTrace::new();
            assert_eq!(contract.get_admin(), AccountId::from([0x01; 32]));
            assert!(contract.is_authorized(AccountId::from([0x01; 32])));
            assert_eq!(contract.next_product_id, 1);
        }

        #[ink::test]
        fn register_product_basic_functionality() {
            let mut contract = PolkaTrace::new();
            let metadata = b"Organic Coffee Beans - Ethiopian Highlands".to_vec();

            let result = contract.register_product(metadata.clone());
            assert!(result.is_ok());

            let product_id = result.unwrap();
            assert_eq!(product_id, 1);

            // Verify product data
            let product = contract.get_product(product_id).unwrap();
            assert_eq!(product.0, manufacturer()); // owner
            assert_eq!(product.1, manufacturer()); // manufacturer
            assert_eq!(product.2, metadata); // metadata
            assert_eq!(product.4, 1); // event_count (created)

            // Verify product appears in owner and manufacturer lists
            assert_eq!(
                contract.get_products_by_owner(manufacturer()),
                vec![product_id]
            );
            assert_eq!(
                contract.get_products_by_manufacturer(manufacturer()),
                vec![product_id]
            );

            // Verify product can be verified
            assert!(contract.verify_product(product_id));
        }

        #[ink::test]
        fn multiple_product_registration() {
            let mut contract = PolkaTrace::new();

            // Register multiple products
            let coffee_metadata = b"Premium Coffee".to_vec();
            let tea_metadata = b"Earl Grey Tea".to_vec();
            let cocoa_metadata = b"Fair Trade Cocoa".to_vec();

            let coffee_id = contract.register_product(coffee_metadata.clone()).unwrap();
            let tea_id = contract.register_product(tea_metadata.clone()).unwrap();
            let cocoa_id = contract.register_product(cocoa_metadata.clone()).unwrap();

            assert_eq!(coffee_id, 1);
            assert_eq!(tea_id, 2);
            assert_eq!(cocoa_id, 3);

            // Verify all products are tracked by manufacturer
            let manufacturer_products = contract.get_products_by_manufacturer(manufacturer());
            assert_eq!(manufacturer_products.len(), 3);
            assert!(manufacturer_products.contains(&coffee_id));
            assert!(manufacturer_products.contains(&tea_id));
            assert!(manufacturer_products.contains(&cocoa_id));

            // Verify each product has correct metadata
            let coffee_product = contract.get_product(coffee_id).unwrap();
            let tea_product = contract.get_product(tea_id).unwrap();
            let cocoa_product = contract.get_product(cocoa_id).unwrap();

            assert_eq!(coffee_product.2, coffee_metadata);
            assert_eq!(tea_product.2, tea_metadata);
            assert_eq!(cocoa_product.2, cocoa_metadata);
        }

        #[ink::test]
        fn authorization_system() {
            let mut contract = PolkaTrace::new();
            let admin = AccountId::from([0x01; 32]);

            // Initially only admin is authorized
            assert!(contract.is_authorized(admin));
            assert!(!contract.is_authorized(distributor()));
            assert!(!contract.is_authorized(logistics_company()));

            // Admin adds authorized accounts
            assert!(contract.add_authorized_account(distributor()).is_ok());
            assert!(contract.add_authorized_account(logistics_company()).is_ok());

            // Verify accounts are now authorized
            assert!(contract.is_authorized(distributor()));
            assert!(contract.is_authorized(logistics_company()));

            // Non-admin cannot add authorized accounts
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor());
            assert_eq!(
                contract.add_authorized_account(retailer()),
                Err(PolkaTraceError::UnauthorizedAccess)
            );

            // Admin can remove authorized accounts
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(admin);
            assert!(contract.remove_authorized_account(distributor()).is_ok());
            assert!(!contract.is_authorized(distributor()));
        }

        #[ink::test]
        fn complete_supply_chain_lifecycle() {
            let mut contract = PolkaTrace::new();

            // Setup: Admin authorizes all supply chain participants
            contract.add_authorized_account(distributor()).unwrap();
            contract
                .add_authorized_account(logistics_company())
                .unwrap();
            contract.add_authorized_account(retailer()).unwrap();
            contract
                .add_authorized_account(quality_inspector())
                .unwrap();
            contract.add_authorized_account(consumer()).unwrap(); // Add consumer authorization

            // Step 1: Manufacturer creates product
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(manufacturer());
            let product_id = contract
                .register_product(b"Luxury Watch - Swiss Made".to_vec())
                .unwrap();

            // Verify initial state
            let product = contract.get_product(product_id).unwrap();
            assert_eq!(product.0, manufacturer()); // owner
            assert_eq!(product.1, manufacturer()); // manufacturer
            assert_eq!(product.4, 1); // event count

            // Step 2: Quality inspection
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(quality_inspector());
            contract.log_event(product_id, EventType::Verified).unwrap(); // 5 = Verified

            // Step 3: Shipped to distributor
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(logistics_company());
            contract.log_event(product_id, EventType::Shipped).unwrap(); // 1 = Shipped

            // Step 4: In transit
            contract
                .log_event(product_id, EventType::InTransit)
                .unwrap(); // 2 = InTransit

            // Step 5: Received by distributor (ownership transfer)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor());
            contract.log_event(product_id, EventType::Received).unwrap(); // 3 = Received

            // Verify ownership transfer
            let product = contract.get_product(product_id).unwrap();
            assert_eq!(product.0, distributor()); // new owner
            assert_eq!(product.1, manufacturer()); // original manufacturer unchanged
            assert_eq!(product.4, 5); // event count increased

            // Verify ownership lists updated
            assert_eq!(
                contract.get_products_by_owner(manufacturer()),
                Vec::<u128>::new()
            );
            assert_eq!(
                contract.get_products_by_owner(distributor()),
                vec![product_id]
            );

            // Step 6: Shipped to retailer
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(logistics_company());
            contract.log_event(product_id, EventType::Shipped).unwrap(); // 1 = Shipped

            // Step 7: Received by retailer (another ownership transfer)
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(retailer());
            contract.log_event(product_id, EventType::Received).unwrap(); // 3 = Received

            // Verify final ownership
            let product = contract.get_product(product_id).unwrap();
            assert_eq!(product.0, retailer()); // final owner
            assert_eq!(product.1, manufacturer()); // original manufacturer unchanged
            assert_eq!(product.4, 7); // total events: created + verified + shipped + transit + received + shipped + received

            // Verify complete ownership history through events
            assert_eq!(
                contract.get_products_by_owner(distributor()),
                Vec::<u128>::new()
            );
            assert_eq!(contract.get_products_by_owner(retailer()), vec![product_id]);

            // Step 8: Final delivery to consumer
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(consumer());
            contract.log_event(product_id, EventType::Received).unwrap(); // 3 = Received (final delivery)

            // Verify final state
            let product = contract.get_product(product_id).unwrap();
            assert_eq!(product.0, consumer()); // final consumer
            assert_eq!(product.4, 8); // total events
            assert_eq!(
                contract.get_products_by_owner(retailer()),
                Vec::<u128>::new()
            );
            assert_eq!(contract.get_products_by_owner(consumer()), vec![product_id]);
        }

        #[ink::test]
        fn multi_product_multi_stakeholder_scenario() {
            let mut contract = PolkaTrace::new();

            // Setup authorization
            contract.add_authorized_account(distributor()).unwrap();
            contract.add_authorized_account(retailer()).unwrap();

            // Manufacturer creates multiple products
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(manufacturer());
            let watch_id = contract.register_product(b"Luxury Watch".to_vec()).unwrap();
            let jewelry_id = contract.register_product(b"Diamond Ring".to_vec()).unwrap();
            let perfume_id = contract
                .register_product(b"Premium Perfume".to_vec())
                .unwrap();

            // Verify manufacturer has all products
            let manufacturer_products = contract.get_products_by_manufacturer(manufacturer());
            assert_eq!(manufacturer_products.len(), 3);
            assert!(manufacturer_products.contains(&watch_id));
            assert!(manufacturer_products.contains(&jewelry_id));
            assert!(manufacturer_products.contains(&perfume_id));

            // Transfer watch and jewelry to distributor
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor());
            contract.log_event(watch_id, EventType::Received).unwrap(); // Received
            contract.log_event(jewelry_id, EventType::Received).unwrap(); // Received

            // Transfer perfume to retailer directly
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(retailer());
            contract.log_event(perfume_id, EventType::Received).unwrap(); // Received

            // Verify ownership distribution
            assert_eq!(
                contract.get_products_by_owner(manufacturer()),
                Vec::<u128>::new()
            );
            assert_eq!(contract.get_products_by_owner(distributor()).len(), 2);
            assert_eq!(contract.get_products_by_owner(retailer()).len(), 1);

            // Distributor transfers watch to retailer
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(retailer());
            contract.log_event(watch_id, EventType::Received).unwrap(); // Received

            // Final verification
            assert_eq!(contract.get_products_by_owner(distributor()).len(), 1); // Only jewelry
            assert_eq!(contract.get_products_by_owner(retailer()).len(), 2); // Watch and perfume

            // Verify each product's event count reflects its journey
            let watch_product = contract.get_product(watch_id).unwrap();
            let jewelry_product = contract.get_product(jewelry_id).unwrap();
            let perfume_product = contract.get_product(perfume_id).unwrap();

            assert_eq!(watch_product.4, 3); // created + received by distributor + received by retailer
            assert_eq!(jewelry_product.4, 2); // created + received by distributor
            assert_eq!(perfume_product.4, 2); // created + received by retailer
        }

        #[ink::test]
        fn unauthorized_access_scenarios() {
            let mut contract = PolkaTrace::new();

            // Register a product as manufacturer
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(manufacturer());
            let product_id = contract.register_product(b"Test Product".to_vec()).unwrap();

            // Unauthorized user tries to log event
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor());
            assert_eq!(
                contract.log_event(product_id, EventType::Shipped),
                Err(PolkaTraceError::UnauthorizedAccess)
            );

            // Unauthorized user tries to log event on non-existent product
            assert_eq!(
                contract.log_event(999, EventType::Shipped),
                Err(PolkaTraceError::UnauthorizedAccess)
            );
        }

        #[ink::test]
        fn product_not_found_scenarios() {
            let mut contract = PolkaTrace::new();

            // Authorize a user
            contract.add_authorized_account(distributor()).unwrap();

            // Try to log event for non-existent product
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor());
            assert_eq!(
                contract.log_event(999, EventType::Shipped),
                Err(PolkaTraceError::ProductNotFound)
            );

            // Verify product doesn't exist
            assert!(!contract.verify_product(999));
            assert!(contract.get_product(999).is_none());
        }

        #[ink::test]
        fn complex_agricultural_supply_chain() {
            let mut contract = PolkaTrace::new();

            // Setup agricultural supply chain participants
            let farmer = account(10);
            let processor = account(11);
            let packager = account(12);
            let distributor_a = account(13);
            let distributor_b = account(14);
            let supermarket = account(15);

            // Authorize all participants
            contract.add_authorized_account(farmer).unwrap();
            contract.add_authorized_account(processor).unwrap();
            contract.add_authorized_account(packager).unwrap();
            contract.add_authorized_account(distributor_a).unwrap();
            contract.add_authorized_account(distributor_b).unwrap();
            contract.add_authorized_account(supermarket).unwrap();

            // Step 1: Farmer harvests and creates batch
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(farmer);
            let batch_id = contract
                .register_product(
                    b"Organic Tomatoes - Batch #2024001 - Farm Location: Napa Valley".to_vec(),
                )
                .unwrap();

            // Step 2: Transport to processor
            contract.log_event(batch_id, EventType::Shipped).unwrap(); // Shipped
            contract.log_event(batch_id, EventType::InTransit).unwrap(); // InTransit

            // Step 3: Processor receives and processes
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(processor);
            contract.log_event(batch_id, EventType::Received).unwrap(); // Received
            contract.log_event(batch_id, EventType::Inspected).unwrap(); // Inspected

            // Step 4: Send to packager
            contract.log_event(batch_id, EventType::Shipped).unwrap(); // Shipped

            // Step 5: Packager receives and packages
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(packager);
            contract.log_event(batch_id, EventType::Received).unwrap(); // Received

            // Step 6: Distribute to multiple distributors
            contract.log_event(batch_id, EventType::Shipped).unwrap(); // Shipped

            // Step 7: Distributor A receives
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor_a);
            contract.log_event(batch_id, EventType::Received).unwrap(); // Received

            // Step 8: Ship to supermarket
            contract.log_event(batch_id, EventType::Shipped).unwrap(); // Shipped

            // Step 9: Supermarket receives final product
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(supermarket);
            contract.log_event(batch_id, EventType::Received).unwrap(); // Received
            contract.log_event(batch_id, EventType::Delivered).unwrap(); // Delivered (final step)

            // Verify complete traceability
            let final_product = contract.get_product(batch_id).unwrap();
            assert_eq!(final_product.0, supermarket); // current owner
            assert_eq!(final_product.1, farmer); // original producer
            assert_eq!(final_product.4, 12); // total events tracked

            // Verify product can be traced back to original farmer
            assert_eq!(
                contract.get_products_by_manufacturer(farmer),
                vec![batch_id]
            );
            assert!(contract.verify_product(batch_id));
        }

        #[ink::test]
        fn pharmaceutical_compliance_tracking() {
            let mut contract = PolkaTrace::new();

            // Pharmaceutical supply chain actors
            let pharma_manufacturer = account(20);
            let quality_control = account(21);
            let pharmaceutical_distributor = account(22);
            let pharmacy = account(23);
            let patient = account(24);

            // Authorize participants
            contract
                .add_authorized_account(pharma_manufacturer)
                .unwrap();
            contract.add_authorized_account(quality_control).unwrap();
            contract
                .add_authorized_account(pharmaceutical_distributor)
                .unwrap();
            contract.add_authorized_account(pharmacy).unwrap();
            contract.add_authorized_account(patient).unwrap();

            // Create pharmaceutical batch
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(pharma_manufacturer);
            let drug_batch_id = contract
                .register_product(
                    b"Aspirin 500mg - Batch #RX2024001 - Mfg Date: 2024-01-15 - Exp: 2026-01-15"
                        .to_vec(),
                )
                .unwrap();

            // Quality control verification
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(quality_control);
            contract
                .log_event(drug_batch_id, EventType::Inspected)
                .unwrap(); // Inspected
            contract
                .log_event(drug_batch_id, EventType::Verified)
                .unwrap(); // Verified

            // Distribution chain
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(pharmaceutical_distributor);
            contract
                .log_event(drug_batch_id, EventType::Received)
                .unwrap(); // Received by distributor
            contract
                .log_event(drug_batch_id, EventType::Shipped)
                .unwrap(); // Shipped to pharmacy

            // Pharmacy receives
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(pharmacy);
            contract
                .log_event(drug_batch_id, EventType::Received)
                .unwrap(); // Received by pharmacy
            contract
                .log_event(drug_batch_id, EventType::Inspected)
                .unwrap(); // Inspected at pharmacy

            // Patient receives prescription
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(patient);
            contract
                .log_event(drug_batch_id, EventType::Received)
                .unwrap(); // Received by patient
            contract
                .log_event(drug_batch_id, EventType::Delivered)
                .unwrap(); // Delivered (dispensed)

            // Verify complete pharmaceutical traceability
            let drug_product = contract.get_product(drug_batch_id).unwrap();
            assert_eq!(drug_product.0, patient); // final recipient
            assert_eq!(drug_product.1, pharma_manufacturer); // original manufacturer
            assert_eq!(drug_product.4, 9); // All compliance steps tracked

            // Critical for pharmaceutical compliance - can trace back to manufacturer
            assert!(contract.verify_product(drug_batch_id));
            assert_eq!(
                contract.get_products_by_manufacturer(pharma_manufacturer),
                vec![drug_batch_id]
            );
        }

        #[ink::test]
        fn stress_test_multiple_products_and_events() {
            let mut contract = PolkaTrace::new();

            // Authorize a distributor
            contract.add_authorized_account(distributor()).unwrap();

            // Create 10 products
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(manufacturer());
            let mut product_ids = Vec::new();

            for i in 0..10 {
                let metadata = format!("Product #{} - Test Item", i).into_bytes();
                let product_id = contract.register_product(metadata).unwrap();
                product_ids.push(product_id);
            }

            // Verify all products were created
            assert_eq!(product_ids.len(), 10);
            assert_eq!(
                contract.get_products_by_manufacturer(manufacturer()).len(),
                10
            );

            // Log multiple events for each product
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor());
            for product_id in &product_ids {
                // Ship and receive each product
                contract.log_event(*product_id, EventType::Shipped).unwrap(); // Shipped
                contract
                    .log_event(*product_id, EventType::InTransit)
                    .unwrap(); // InTransit
                contract
                    .log_event(*product_id, EventType::Received)
                    .unwrap(); // Received
            }

            // Verify all products transferred to distributor
            assert_eq!(contract.get_products_by_owner(distributor()).len(), 10);
            assert_eq!(contract.get_products_by_owner(manufacturer()).len(), 0);

            // Verify event counts
            for product_id in &product_ids {
                let product = contract.get_product(*product_id).unwrap();
                assert_eq!(product.4, 4); // created + shipped + transit + received
                assert_eq!(product.0, distributor()); // current owner
                assert_eq!(product.1, manufacturer()); // original manufacturer
            }
        }

        #[ink::test]
        fn overflow_protection_tests() {
            let mut contract = PolkaTrace::new();

            // Test product ID overflow protection
            contract.next_product_id = u128::MAX;
            let metadata = b"Test Product".to_vec();

            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(manufacturer());
            let result = contract.register_product(metadata);
            assert!(result.is_ok());

            // Should handle overflow gracefully
            assert_eq!(contract.next_product_id, u128::MAX);

            // Test event count overflow protection
            let product_id = result.unwrap();
            contract.product_event_count.insert(product_id, &u32::MAX);

            contract.add_authorized_account(distributor()).unwrap();
            ink::env::test::set_caller::<ink::env::DefaultEnvironment>(distributor());

            // Should handle event count overflow gracefully
            let log_result = contract.log_event(product_id, EventType::Shipped);
            assert!(log_result.is_ok());

            let product = contract.get_product(product_id).unwrap();
            assert_eq!(product.4, u32::MAX); // Should not overflow
        }
    }
}
