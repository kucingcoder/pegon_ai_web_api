use rocket::catch;
use rocket::response::Redirect;

#[catch(401)]
pub fn add_in_unauthorized_redirect() -> Redirect {
    Redirect::to("/add-in/login/add-in-auth-view")
}
