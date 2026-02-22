use crate::models::sea_orm_active_enums::{Category, Status as TransactionStatus};
use chrono::{FixedOffset, NaiveDateTime, TimeZone, Utc};
use reqwest::Client;
use rocket::http::Status;
use rocket::serde::json;
use rocket::serde::json::{Json, serde_json::Value, serde_json::json};
use rocket::{State, get, post};
use sea_orm::prelude::{DateTimeUtc, Expr};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use std::env;
use uuid::Uuid;

use crate::middlewares::auth_guard::AuthenticatedUser;
use crate::models::{transaction_model, user_model};

#[post("/transaction/upgrade-to-premium")]
pub async fn upgrade_to_premium(
    db: &State<DatabaseConnection>,
    client: &State<Client>,
    auth: AuthenticatedUser,
) -> Result<json::Value, (Status, String)> {
    let db = db as &DatabaseConnection;

    let (email, full_name, category): (String, String, Category) =
        user_model::Entity::find_by_id(auth.id)
            .select_only()
            .column(user_model::Column::Email)
            .column(user_model::Column::FullName)
            .column(user_model::Column::Category)
            .into_tuple()
            .one(db)
            .await
            .map_err(|e| (Status::InternalServerError, e.to_string()))?
            .ok_or((Status::NotFound, "User not found".to_string()))?;

    if category == Category::Premium {
        return Err((Status::BadRequest, "User already premium".to_string()));
    }

    let midtrans_url = env::var("MIDTRANS_URL").expect("MIDTRANS_URL must be set");
    let midtrans_server_key =
        env::var("MIDTRANS_SERVER_KEY").expect("MIDTRANS_SERVER_KEY must be set");
    let id_trasaction = Uuid::new_v4();

    // used shared client
    let resp = client
        .post(midtrans_url + "/v2/charge")
        .basic_auth(midtrans_server_key, Some(""))
        .header("User-Agent", "actix-web/3.0")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&json!({
          "customer_details": {
                "email": email,
                "first_name": full_name,
            },
          "payment_type": "qris",
          "qris": {
            "acquirer": "gopay"
          },
          "item_details": [
            {
              "id": "1",
              "price": 30000,
              "quantity": 1,
              "name": "Pegon AI Premium 1 Bulan"
            },
            {
              "id": "2",
              "price": 250,
              "quantity": 1,
              "name": "biaya qris (0.7%)"
            }
          ],
          "transaction_details": {
            "order_id": id_trasaction.to_string(),
            "gross_amount": 30250
          },
          "custom_expiry": {
            "expiry_duration": 15,
            "unit": "minute"
          }
        }))
        .send()
        .await
        .map_err(|_| (Status::InternalServerError, "Midtrans Error".to_string()))?;

    if !resp.status().is_success() {
        let response_text = resp.text().await.map_err(|e| {
            (
                Status::InternalServerError,
                format!("Gagal baca body: {}", e),
            )
        })?;

        // 3. PRINT hasil JSON mentah ke terminal/console (Debug)
        println!("Midtrans Error Response Raw: {}", response_text);
        return Err((Status::InternalServerError, "Midtrans Error".to_string()));
    }

    let data: Value = resp
        .json()
        .await
        .map_err(|_| (Status::InternalServerError, "Midtrans Error".to_string()))?;

    let midtrans_id_str = data["transaction_id"].as_str().ok_or((
        Status::InternalServerError,
        "Missing transaction_id".to_string(),
    ))?;

    let midtrans_uuid = Uuid::parse_str(midtrans_id_str).map_err(|_| {
        (
            Status::InternalServerError,
            "Invalid Midtrans UUID format".to_string(),
        )
    })?;

    let qr_code_url = data["actions"][0]["url"]
        .as_str()
        .or_else(|| data["action"][0]["url"].as_str())
        .unwrap_or("")
        .to_string();

    let expired_at_str = data["expiry_time"].as_str().ok_or((
        Status::InternalServerError,
        "Missing expiry_time".to_string(),
    ))?;

    let naive_date =
        NaiveDateTime::parse_from_str(expired_at_str, "%Y-%m-%d %H:%M:%S").map_err(|e| {
            (
                Status::InternalServerError,
                format!("Gagal parse tanggal: {}", e),
            )
        })?;

    let wib_offset = FixedOffset::east_opt(7 * 3600).expect("Error offset");

    let expired_at_utc: DateTimeUtc = wib_offset
        .from_local_datetime(&naive_date)
        .unwrap()
        .with_timezone(&Utc);

    // save transaction
    let transaction = transaction_model::ActiveModel {
        id: Set(id_trasaction),
        id_user: Set(auth.id),
        id_midtrans: Set(midtrans_uuid),
        title: Set("Pegon AI Premium 1 Bulan".to_string()),
        value: Set(30250),
        qr_code: Set(qr_code_url),
        status: Set(TransactionStatus::Pending),
        expired_at: Set(Some(expired_at_utc)),
        ..Default::default()
    };

    transaction
        .insert(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?;

    Ok(json!({"id": id_trasaction}))
}

#[get("/transaction/history?<page>&<limit>")]
pub async fn history(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    page: Option<u64>,
    limit: Option<u64>,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let page = page.unwrap_or(1);
    let limit = limit.unwrap_or(10);

    let paginator = transaction_model::Entity::find()
        .filter(transaction_model::Column::IdUser.eq(auth.id))
        .order_by_desc(transaction_model::Column::CreatedAt)
        .select_only()
        .column(transaction_model::Column::Id)
        .column(transaction_model::Column::Title)
        .column(transaction_model::Column::Value)
        .column(transaction_model::Column::CreatedAt)
        .column(transaction_model::Column::ExpiredAt)
        .column(transaction_model::Column::Status)
        .into_json()
        .paginate(db, limit);

    let total_pages = paginator.num_pages().await.map_err(|_| {
        (
            Status::InternalServerError,
            "Gagal menghitung halaman".to_string(),
        )
    })?;

    let pagination_result = paginator.fetch_page(page.saturating_sub(1)).await;

    match pagination_result {
        Ok(items) => Ok(Json(json!({
            "data": items,
            "meta": {
                "current_page": page,
                "per_page": limit,
                "total_pages": total_pages
            }
        }))),
        Err(_) => Err((
            Status::InternalServerError,
            "Gagal mengambil data riwayat".to_string(),
        )),
    }
}

#[get("/transaction/history/info?<id>")]
pub async fn info(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    id: String,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let parsed_id = Uuid::parse_str(&id)
        .map_err(|_| (Status::BadRequest, "Format ID tidak valid".to_string()))?;

    let transaction = transaction_model::Entity::find()
        .filter(transaction_model::Column::IdUser.eq(auth.id))
        .filter(transaction_model::Column::Id.eq(parsed_id))
        .one(db)
        .await
        .map_err(|_| (Status::InternalServerError, "Db Error".to_string()))?
        .ok_or((Status::NotFound, "Data not found".to_string()))?;

    Ok(Json(json!({
        "id": transaction.id,
        "title": transaction.title,
        "value": transaction.value,
        "qr_code": transaction.qr_code,
        "created_at": transaction.created_at,
        "expired_at": transaction.expired_at,
        "status": transaction.status
    })))
}

#[get("/transaction/history/status?<id>")]
pub async fn status(
    db: &State<DatabaseConnection>,
    auth: AuthenticatedUser,
    id: String,
) -> Result<Json<Value>, (Status, String)> {
    let db = db as &DatabaseConnection;

    let parsed_id = Uuid::parse_str(&id)
        .map_err(|_| (Status::BadRequest, "Format ID tidak valid".to_string()))?;

    let status: TransactionStatus = transaction_model::Entity::find()
        .filter(transaction_model::Column::IdUser.eq(auth.id))
        .filter(transaction_model::Column::Id.eq(parsed_id))
        .select_only()
        .column(transaction_model::Column::Status)
        .into_tuple()
        .one(db)
        .await
        .map_err(|e| (Status::InternalServerError, e.to_string()))?
        .ok_or((Status::NotFound, "Data not found".to_string()))?;

    Ok(Json(json!({"status": status})))
}

use sha2::{Sha512, Digest};

#[derive(Debug, Deserialize, Serialize)]
pub struct NotificationRequest {
    pub transaction_id: String,
    pub transaction_status: String,
    pub order_id: String,
    pub status_code: String,
    pub gross_amount: String,
    pub signature_key: String,
}

#[post("/transaction/notification", data = "<data>")]
pub async fn notification(
    db: &State<DatabaseConnection>,
    data: Json<NotificationRequest>,
) -> Result<Json<Value>, Status> {
    let midtrans_server_key = env::var("MIDTRANS_SERVER_KEY").expect("MIDTRANS_SERVER_KEY must be set");

    let signature_payload = format!(
        "{}{}{}{}",
        data.order_id, data.status_code, data.gross_amount, midtrans_server_key
    );

    let mut hasher = Sha512::new();
    hasher.update(signature_payload);
    let result = hasher.finalize();
    let calculated_signature = hex::encode(result);

    if calculated_signature != data.signature_key {
        println!("Invalid Signature: {}", calculated_signature); 
        return Err(Status::Forbidden);
    }

    let parsed_id = Uuid::parse_str(&data.transaction_id).map_err(|_| Status::BadRequest)?;

    let new_status = match data.transaction_status.as_str() {
        "settlement" | "capture" => TransactionStatus::Success,
        "pending" => TransactionStatus::Pending,
        _ => TransactionStatus::Canceled,
    };

    let transaction_data = transaction_model::Entity::find()
        .filter(transaction_model::Column::IdMidtrans.eq(parsed_id))
        .one(db as &DatabaseConnection)
        .await
        .map_err(|_| Status::InternalServerError)? // DB Error
        .ok_or(Status::NotFound)?; // Data tidak ketemu

    let user_id = transaction_data.id_user;

    if transaction_data.status != new_status {
        let mut transaction_active: transaction_model::ActiveModel = transaction_data.into();
        transaction_active.status = Set(new_status.clone());
        transaction_active
            .update(db as &DatabaseConnection)
            .await
            .map_err(|_| Status::InternalServerError)?;
    }

    if new_status == TransactionStatus::Success {
        let new_expired_at = Utc::now() + chrono::Duration::days(30);

        user_model::Entity::update_many()
            .col_expr(user_model::Column::Category, Expr::value(Category::Premium))
            .col_expr(user_model::Column::ExpiredAt, Expr::value(new_expired_at))
            .filter(user_model::Column::Id.eq(user_id))
            .exec(db as &DatabaseConnection)
            .await
            .map_err(|_| Status::InternalServerError)?;
    }

    Ok(Json(json!({"status": "ok"})))
}
