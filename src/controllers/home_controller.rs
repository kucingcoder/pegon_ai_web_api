use rocket_dyn_templates::{Template, context};

#[get("/")]
pub fn index() -> Template {
    Template::render("landing_home", context! {})
}

#[get("/landing")]
pub fn landing() -> Template {
    Template::render("landing_home", context! {})
}

#[get("/landing/app")]
pub fn app() -> Template {
    Template::render("landing_app", context! {})
}

#[get("/landing/contact")]
pub fn contact() -> Template {
    Template::render("landing_contact", context! {})
}
