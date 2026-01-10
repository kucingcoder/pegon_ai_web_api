use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use uuid::Uuid;

pub struct AuthenticatedUser {
    pub id: Uuid,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthenticatedUser {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let cookies = req.cookies();

        if let Some(cookie) = cookies.get_private("session") {
            if let Ok(uuid) = Uuid::parse_str(cookie.value()) {
                return Outcome::Success(AuthenticatedUser { id: uuid });
            }
        }

        Outcome::Error((Status::Unauthorized, ()))
    }
}
