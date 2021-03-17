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

use serde_with::DisplayFromStr;
use wallet::Slice32;

#[serde_as]
#[derive(
    Serialize,
    Deserialize,
    Copy,
    Clone,
    Ord,
    PartialOrd,
    Eq,
    PartialEq,
    Hash,
    Default,
    Debug,
    Display,
    StrictEncode,
    StrictDecode,
)]
#[display("<internal state data>")]
pub struct State {
    #[serde_as(as = "DisplayFromStr")]
    pub mu_sig: Slice32,
}
