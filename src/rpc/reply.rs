// Keyring: private/public key managing service
// Written in 2020 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the AGPL License
// along with this software.
// If not, see <https://www.gnu.org/licenses/agpl-3.0-standalone.html>.

use internet2::presentation;
use microservices::{rpc, rpc_connection};

use crate::Error;

#[derive(Clone, Debug, Display, LnpApi)]
#[encoding_crate(lnpbp::strict_encoding)]
#[lnp_api(encoding = "strict")]
#[non_exhaustive]
pub enum Reply {
    #[lnp_api(type = 0x0100)]
    #[display("success()")]
    Success,

    #[lnp_api(type = 0x0102)]
    #[display("failure({0})")]
    Failure(microservices::rpc::Failure),
}

impl rpc_connection::Reply for Reply {}

impl From<presentation::Error> for Reply {
    fn from(err: presentation::Error) -> Self {
        // TODO: Save error code taken from `Error::to_value()` after
        //       implementation of `ToValue` trait and derive macro for enums
        Reply::Failure(microservices::rpc::Failure {
            code: 0,
            info: format!("{}", err),
        })
    }
}

impl From<Error> for rpc::Failure {
    fn from(err: Error) -> Self {
        rpc::Failure {
            code: 1, // Error from LNPD
            info: err.to_string(),
        }
    }
}

impl From<rpc::Failure> for Error {
    fn from(fail: rpc::Failure) -> Self {
        Error::FailedRequest(fail.to_string())
    }
}
