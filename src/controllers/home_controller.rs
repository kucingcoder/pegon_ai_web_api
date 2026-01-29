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

#[get("/landing/privacy")]
pub fn privacy() -> Template {
    Template::render("landing_privacy", context! {})
}

#[get("/landing/terms")]
pub fn terms() -> Template {
    Template::render("landing_terms", context! {})
}

#[get("/landing/license")]
pub fn license() -> Template {
    Template::render("landing_license", context! {})
}
