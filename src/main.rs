#[macro_use]
extern crate rocket;
use rocket::fs::FileServer;
use rocket_dyn_templates::Template;
use sea_orm::Database;
mod controllers;
mod middlewares;
mod models;

#[launch]
async fn rocket() -> _ {
    // load .env
    dotenvy::dotenv().ok();

    // konek ke database
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL wajib ada di .env");
    let db = Database::connect(&db_url)
        .await
        .expect("Gagal konek ke Database");

    // buat folder images kalo belum ada
    std::fs::create_dir_all("images").expect("Gagal buat folder images");
    std::fs::create_dir_all("images/photo_profiles").expect("Gagal buat folder photo_profiles");
    std::fs::create_dir_all("images/transliterations").expect("Gagal buat folder transliterations");
    std::fs::create_dir_all("images/temp").expect("Gagal buat folder temp");

    // build rocket
    rocket::build()
        // database
        .manage(db)
        // templates
        .attach(Template::fairing())
        // redirect login
        .register(
            "/add-in",
            catchers![crate::middlewares::catchers::add_in_unauthorized_redirect],
        )
        // file
        .mount("/static", FileServer::from("static"))
        .mount("/images", FileServer::from("images"))
        // landing page
        .mount("/", routes![controllers::home_controller::index])
        // add-in routes
        .mount(
            "/add-in",
            routes![
                controllers::auth_controller::login_add_in_auth_view,
                controllers::auth_controller::login_add_in_auth_handle,
                controllers::auth_controller::logout_add_in,
                controllers::text_transliteration_controller::add_in_transliterate_view,
                controllers::text_transliteration_controller::add_in_transliterate_handle
            ],
        )
        // api routes
        .mount(
            "/api",
            routes![
                controllers::auth_controller::login,
                controllers::auth_controller::logout,
                controllers::user_controller::get_profile,
                controllers::user_controller::get_profile_detail,
                controllers::user_controller::update_profile,
                controllers::text_transliteration_controller::transliterate,
                controllers::image_transliteration_controller::transliterate,
                controllers::image_transliteration_controller::history,
                controllers::image_transliteration_controller::read,
                controllers::image_transliteration_controller::update_title,
                controllers::check_controller::check_level_stage,
                controllers::check_controller::check_update_level_stage,
                controllers::check_controller::check_read,
                controllers::check_controller::check_write,
                controllers::transaction_controller::upgrade_to_premium,
                controllers::transaction_controller::history,
                controllers::transaction_controller::info,
                controllers::transaction_controller::status,
                controllers::transaction_controller::notification
            ],
        )
}
