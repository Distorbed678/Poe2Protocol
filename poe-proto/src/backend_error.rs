//! Result codes of the login connection: rows of the client's `Data/Balance/BackendErrors.dat`.
//!
//! The client has one code table for every result code on the login connection
//! (`0x0004` AuthenticationResult, `0x0013` CharacterCreated, `0x000d` DeleteCharacterResult):
//! **code = row index + 1**, and it prints the row's `Text` for a code it shows as an error
//! (`BackendError_GetText` `0x1425cb780`; see `docs/protocol/login.md`). The packets carry no
//! text, so the server only picks the code.
//!
//! The table is game data, so this module names the codes the server uses in one place, each
//! with the row's `Id`. The numbers are a convenience for code that has no game data at hand
//! (unit tests, the wire layer); the server checks them against the real table when it
//! starts (`login-server`, `codes::verify`), so a client update that moves rows is caught
//! there instead of showing the wrong text.

/// One `BackendErrors` row: the wire code and the row's `Id` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendError {
    /// The `u16` on the wire (`row + 1`).
    pub code: u16,
    /// `BackendErrors.Id` of row `code - 1`.
    pub id: &'static str,
}

macro_rules! backend_errors {
    ($($(#[$m:meta])* $name:ident = $code:literal, $id:literal;)*) => {
        $($(#[$m])* pub const $name: BackendError = BackendError { code: $code, id: $id };)*
        /// Every code named here (for checking them against the table).
        pub const ALL: &[BackendError] = &[$($name),*];
    };
}

backend_errors! {
    /// Login: no such account.
    ACCOUNT_DOES_NOT_EXIST = 5, "AccountDoesNotExist";
    /// Login: wrong password.
    INVALID_PASSWORD = 6, "InvalidPassword";
    /// Login: the client is not the build the server speaks ("There has been a patch that you
    /// need to update to. Please restart Path of Exile."). Used when the `client_hash` of the
    /// login is not a known value of the server's version.
    INVALID_PROTOCOL_VERSION = 8, "InvalidProtocolVersion";
    /// Login: this kind of login is switched off.
    NOT_ALLOWED_GAME_LOGIN = 9, "NotAllowedGameLogin";
    /// Login: the client **deletes its saved credentials file** (not on Steam), then shows
    /// the popup. Used for tokens that can never work here.
    UNEXPECTED_DISCONNECT = 15, "UnexpectedDisconnect";
    /// Delete: no character of that name.
    CHARACTER_DOES_NOT_EXIST = 16, "CharacterDoesNotExist";
    /// Delete: the character belongs to another account.
    NOT_CHARACTER_OWNER = 17, "NotCharacterOwner";
    /// Any: the store failed.
    DATABASE_ERROR = 19, "DatabaseError";
    /// Create: the name is taken (names are global).
    CHARACTER_NAME_ALREADY_EXISTS = 21, "CharacterNameAlreadyExists";
    /// Create: characters other than letters (and underscores) in the name.
    CHARACTER_NAME_INVALID = 22, "CharacterNameInvalid";
    /// Create: name too short (or empty).
    CHARACTER_NAME_TOO_SHORT = 23, "CharacterNameTooShort";
    /// Create: name too long.
    CHARACTER_NAME_TOO_LONG = 24, "CharacterNameTooLong";
    /// Create: unknown class / start state.
    CHARACTER_INVALID_CLASS = 25, "CharacterInvalidClass";
    /// Create: unknown league.
    LEAGUE_DOES_NOT_EXIST = 26, "LeagueDoesNotExist";
    /// Create: bad flags (hardcore, SSF, ...).
    INVALID_CHARACTER_FLAGS = 27, "InvalidCharacterFlags";
    /// Create: no free character slot.
    TOO_MANY_CHARACTERS = 49, "TooManyCharacters";
    /// Login: banned (our disabled accounts).
    ACCOUNT_BANNED = 71, "AccountBanned";
    /// Login: too many password attempts ("You have attempted to log in too often within a
    /// short time period. Try again later.").
    LOGIN_RATE_LIMITED = 73, "LoginRateLimited";
    /// Login: "Your saved credentials are out of date. Please re-enter your password."
    /// The client keeps its credentials file, so the token is retried on the next start.
    SAVED_CREDENTIALS_BAD = 97, "SavedCredentialsBad";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_row_plus_one() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(a.code >= 1, "{}: row 0 is code 1", a.id);
            assert!(
                ALL[i + 1..]
                    .iter()
                    .all(|b| b.code != a.code && b.id != a.id),
                "{}",
                a.id
            );
        }
        assert_eq!(CHARACTER_NAME_ALREADY_EXISTS.code, 0x15);
        assert_eq!(TOO_MANY_CHARACTERS.code, 0x31);
        assert_eq!(UNEXPECTED_DISCONNECT.code, 0x0f);
        assert_eq!(INVALID_PROTOCOL_VERSION.code, 8);
    }
}
