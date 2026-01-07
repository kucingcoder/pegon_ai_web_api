use super::sea_orm_active_enums::Category;
use super::sea_orm_active_enums::Gender;
use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Binary(16)")]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub email: String,
    #[sea_orm(unique)]
    pub photo_profile: Option<String>,
    pub full_name: String,
    pub gender: Gender,
    pub date_of_birth: Date,
    pub category: Category,
    pub learning_level: i32,
    pub learning_stage_level: i32,
    pub created_at: Option<DateTimeUtc>,
    pub updated_at: Option<DateTimeUtc>,
    pub expired_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::image_transliteration_model::Entity")]
    ImageTransliterations,
    #[sea_orm(has_many = "super::text_transliteration_model::Entity")]
    TextTransliterations,
    #[sea_orm(has_many = "super::transaction_model::Entity")]
    Transactions,
}

impl Related<super::image_transliteration_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ImageTransliterations.def()
    }
}

impl Related<super::text_transliteration_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TextTransliterations.def()
    }
}

impl Related<super::transaction_model::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Transactions.def()
    }
}

#[async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let now = Utc::now();

        // Logika saat Insert (Data Baru)
        if insert {
            // 1. Generate UUID otomatis jika belum di-set manual
            if self.id.is_not_set() {
                self.id = Set(Uuid::new_v4());
            }

            // 2. Set created_at ke waktu sekarang
            self.created_at = Set(Some(now));
        }

        // Logika saat Insert DAN Update
        // 3. Selalu update updated_at ke waktu sekarang
        self.updated_at = Set(Some(now));

        Ok(self)
    }
}
