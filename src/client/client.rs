// Citadel: Bitcoin, LN & RGB wallet runtime
// Written in 2021 by
//     Dr. Maxim Orlovsky <orlovsky@mycitadel.io>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the AGPL License
// along with this software.
// If not, see <https://www.gnu.org/licenses/agpl-3.0-standalone.html>.

use std::convert::TryFrom;
use std::str::FromStr;

use bitcoin::Txid;
use internet2::zmqsocket::{self, ZmqType};
use internet2::{
    session, CreateUnmarshaller, PlainTranscoder, Session, TypedEnum,
    Unmarshall, Unmarshaller,
};
use invoice::{AssetClass, Beneficiary, Invoice};
use lnpbp::chain::{AssetId, Chain};
use lnpbp::client_side_validation::CommitConceal;
use microservices::rpc::Failure;
use rgb::{AtomicValue, Consignment, Genesis};
use wallet::bip32::{PubkeyChain, UnhardenedIndex};
use wallet::descriptor::{self, ContentType};
use wallet::script::PubkeyScript;
use wallet::Psbt;

use super::Config;
use crate::model::ContractId;
use crate::rpc::{message, Reply, Request};
use crate::Error;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum InvoiceType {
    AddressUtxo,
    Descriptor,
    Psbt,
}
#[repr(C)]
pub struct Client {
    config: Config,
    session_rpc: session::Raw<PlainTranscoder, zmqsocket::Connection>,
    unmarshaller: Unmarshaller<Reply>,
}

impl Client {
    pub fn with(config: Config) -> Result<Self, Error> {
        debug!("Initializing runtime");
        debug!("Connecting to citadel daemon at {}", config.rpc_endpoint);
        let session_rpc = session::Raw::with_zmq_unencrypted(
            ZmqType::Req,
            &config.rpc_endpoint,
            None,
            None,
        )?;
        Ok(Self {
            config,
            session_rpc,
            unmarshaller: Reply::create_unmarshaller(),
        })
    }

    pub fn request(&mut self, request: Request) -> Result<Reply, Error> {
        debug!("Sending request to the server: {:?}", request);
        let data = request.serialize();
        debug!("Raw request data ({} bytes): {:02X?}", data.len(), data);
        self.session_rpc.send_raw_message(&data)?;
        debug!("Awaiting reply");
        let raw = self.session_rpc.recv_raw_message()?;
        debug!("Got reply ({} bytes), parsing: {:02X?}", raw.len(), raw);
        let reply = self.unmarshaller.unmarshall(&raw)?;
        debug!("Reply: {:?}", reply);
        Ok((&*reply).clone())
    }
}

impl Client {
    pub fn contract_list(&mut self) -> Result<Reply, Error> {
        self.request(Request::ListContracts)
    }

    pub fn single_sig_create(
        &mut self,
        name: impl ToString,
        pubkey_chain: PubkeyChain,
        category: ContentType,
    ) -> Result<Reply, Error> {
        self.request(Request::CreateSingleSig(message::SingleSigInfo {
            name: name.to_string(),
            pubkey_chain,
            category,
        }))
    }

    pub fn contract_operations(
        &mut self,
        contract_id: ContractId,
    ) -> Result<Reply, Error> {
        self.request(Request::ContractOperations(contract_id))
    }

    pub fn contract_rename(
        &mut self,
        contract_id: ContractId,
        name: impl ToString,
    ) -> Result<Reply, Error> {
        self.request(Request::RenameContract(message::RenameContractRequest {
            contract_id,
            name: name.to_string(),
        }))
    }

    pub fn contract_delete(
        &mut self,
        contract_id: ContractId,
    ) -> Result<Reply, Error> {
        self.request(Request::DeleteContract(contract_id))
    }

    pub fn contract_balance(
        &mut self,
        contract_id: ContractId,
        rescan: bool,
        lookup_depth: u8,
    ) -> Result<Reply, Error> {
        if rescan {
            self.request(Request::SyncContract(message::SyncContractRequest {
                contract_id,
                lookup_depth,
            }))
        } else {
            self.request(Request::ContractUnspent(contract_id))
        }
    }

    pub fn address_list(
        &mut self,
        contract_id: ContractId,
        rescan: bool,
        lookup_depth: u8,
    ) -> Result<Reply, Error> {
        if rescan {
            self.request(Request::SyncContract(
                message::SyncContractRequest {
                    contract_id,
                    lookup_depth,
                },
            ))?;
        }
        self.request(Request::UsedAddresses(contract_id))
    }

    pub fn address_create(
        &mut self,
        contract_id: ContractId,
        index: Option<UnhardenedIndex>,
        mark_used: bool,
        legacy: bool,
    ) -> Result<Reply, Error> {
        self.request(Request::NextAddress(message::NextAddressRequest {
            contract_id,
            index,
            legacy,
            mark_used,
        }))
    }

    pub fn invoice_create(
        &mut self,
        category: InvoiceType,
        contract_id: ContractId,
        asset_id: Option<rgb::ContractId>,
        amount: AtomicValue,
        merchant: Option<impl ToString>,
        purpose: Option<impl ToString>,
        mark_used: bool,
        legacy: bool,
    ) -> Result<Invoice, Error> {
        let mut asset_id = asset_id.map(AssetId::from);
        let (beneficiary, reveal_data) = match (category, asset_id) {
            (InvoiceType::AddressUtxo, Some(asset_id)) => {
                let seal =
                    match self.request(Request::BlindUtxo(contract_id))? {
                        Reply::BlindUtxo(seal) => seal,
                        Reply::Failure(failure) => Err(failure)?,
                        _ => Err(Error::UnexpectedApi)?,
                    };
                (Beneficiary::BlindUtxo(seal.commit_conceal()), Some(seal))
            }
            (InvoiceType::AddressUtxo, None) => {
                let address = match self.request(Request::NextAddress(
                    message::NextAddressRequest {
                        contract_id,
                        index: None,
                        legacy,
                        mark_used,
                    },
                ))? {
                    Reply::AddressDerivation(ad) => ad.address,
                    Reply::Failure(failure) => Err(failure)?,
                    _ => Err(Error::UnexpectedApi)?,
                };
                if address.network != bitcoin::Network::Bitcoin {
                    asset_id = Some(Chain::from(address.network).native_asset())
                }
                (Beneficiary::Address(address), None)
            }
            _ => unimplemented!(),
        };
        let mut inv = Invoice::new(beneficiary, Some(amount), asset_id);
        if let Some(merchant) = merchant {
            inv.set_merchant(merchant.to_string());
        }
        if let Some(purpose) = purpose {
            inv.set_purpose(purpose.to_string());
        }
        let _ =
            self.request(Request::AddInvoice(message::AddInvoiceRequest {
                invoice: inv.clone(),
                source_info: bmap! { contract_id => reveal_data },
            }))?;
        Ok(inv)
    }

    pub fn invoice_list(
        &mut self,
        contract_id: ContractId,
    ) -> Result<Reply, Error> {
        self.request(Request::ListInvoices(contract_id))
    }

    pub fn invoice_pay(
        &mut self,
        contract_id: ContractId,
        invoice: Invoice,
        amount: Option<u64>,
        fee: u64,
        giveaway: Option<u64>,
    ) -> Result<message::PreparedTransfer, Error> {
        debug!(
            "Doing transfer for invoice {} using wallet {} with fee {}",
            invoice, contract_id, fee
        );
        debug!("Parsed invoice: {:#?}", invoice);

        let transfer_info = if let Some(asset_id) = invoice.rgb_asset() {
            debug!("Performing transfer in {} assets", asset_id);
            message::TransferInfo::Rgb {
                contract_id: asset_id,
                receiver: match invoice.beneficiary() {
                    Beneficiary::Address(_) => Err(Error::ServerFailure(Failure {
                        code: 0,
                        info: s!("Malformed invoice: RGB assets can't be paid to an address")
                    }))?,
                    Beneficiary::BlindUtxo(hash) => message::RgbReceiver::BlindUtxo(*hash),
                    /* TODO: Need a derivation function to support descriptor-based invoices
                    Beneficiary::Descriptor(..) => message::RgbReceiver::Descriptor {
                        descriptor,
                        giveaway: giveaway.ok_or(Error::ServerFailure(Failure {
                            code: 0,
                            info: s!("Giveaway amount is required for descriptor-based RGB payments")
                        }))?
                    },
                     */
                    _ => unimplemented!()
                }
            }
        } else {
            let (descriptor, chain) = match invoice.beneficiary() {
                Beneficiary::Address(address) => {
                    trace!("Paying to bitcoin address {}", address);
                    (
                        descriptor::Compact::try_from(PubkeyScript::from(
                            address.script_pubkey(),
                        ))
                            .expect("Address is always parsable as a descriptor"),
                        Some(Chain::from(address.network)),
                    )
                },
                Beneficiary::BlindUtxo(hash) => Err(Error::ServerFailure(Failure {
                    code: 0,
                    info: s!("Malformed invoice: bitcoins can't be paid to an existing UTXO")
                }))?,
                Beneficiary::Descriptor(d) => {
                    unimplemented!();
                    /*
                    Address::from_script(
                        &descriptor.script_pubkey(),
                        bitcoin::Network::Bitcoin,
                    ).expect("We do not support descriptors not representable by address yet")
                     */
                }
                _ => unimplemented!(),
            };

            debug!(
                "Paying to descriptor {} using {} chain",
                descriptor,
                chain
                    .as_ref()
                    .map(Chain::to_string)
                    .unwrap_or(s!("default"))
            );

            match invoice.classify_asset(chain) {
                AssetClass::Native => {
                    trace!("Performing native bitcoin transfer");
                    message::TransferInfo::Bitcoin(descriptor)
                }
                AssetClass::Rgb(asset_id) => unreachable!(),
                AssetClass::InvalidNativeChain => {
                    Err(Error::ServerFailure(Failure {
                        code: 0,
                        info: s!(
                            "Current network does not match invoice network"
                        ),
                    }))?
                }
                _ => Err(Error::ServerFailure(Failure {
                    code: 0,
                    info: s!("Unsupported asset type"),
                }))?,
            }
        };

        match self.request(Request::ComposeTransfer(message::ComposeTransferRequest {
            pay_from: contract_id,
            bitcoin_fee: fee,
            asset_value: invoice.amount().atomic_value().or(amount).ok_or(Error::ServerFailure(Failure {
                code: 0,
                info: s!("Amount must be specified for invoices if they do not provide default amount value")
            }))?,
            transfer_info,
            invoice
        }))? {
            Reply::PreparedPayment(payment_info) => Ok(payment_info),
            Reply::Failure(failure) => Err(failure.into()),
            _ => Err(Error::UnexpectedApi),
        }
    }

    pub fn finalize_publish_psbt(&mut self, psbt: Psbt) -> Result<Txid, Error> {
        let txid = psbt.global.unsigned_tx.txid();
        match self.request(Request::FinalizeTransfer(psbt))? {
            Reply::Success => Ok(txid),
            Reply::Failure(failure) => Err(failure.into()),
            _ => Err(Error::UnexpectedApi),
        }
    }

    pub fn invoice_accept(
        &mut self,
        consignment: Consignment,
    ) -> Result<rgb::validation::Status, Error> {
        match self.request(Request::AcceptTransfer(consignment))? {
            Reply::Validation(status) => Ok(status),
            Reply::Failure(failure) => Err(failure.into()),
            _ => Err(Error::UnexpectedApi),
        }
    }

    pub fn asset_list(&mut self) -> Result<Reply, Error> {
        self.request(Request::ListAssets)
    }

    pub fn asset_import(
        &mut self,
        genesis_bech: impl AsRef<str>,
    ) -> Result<Reply, Error> {
        let genesis =
            Genesis::from_str(genesis_bech.as_ref()).map_err(|err| {
                error!("Wrong genesis data: {}", err);
                Error::EmbeddedNodeInitError
            })?;
        self.request(Request::ImportAsset(genesis))
    }
}
