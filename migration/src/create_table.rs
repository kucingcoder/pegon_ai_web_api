use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Users::Id).uuid().not_null().primary_key())
                    .col(
                        ColumnDef::new(Users::Email)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Users::PhotoProfile)
                            .string()
                            .null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Users::FullName).string().not_null())
                    .col(
                        ColumnDef::new(Users::Gender)
                            .enumeration(
                                GenderEnum::Table,
                                [
                                    GenderEnum::Male,
                                    GenderEnum::Female
                                ],
                            )
                            .not_null(),
                    )
                    .col(ColumnDef::new(Users::DateOfBirth).date().not_null())
                    .col(
                        ColumnDef::new(Users::Category)
                            .enumeration(
                                CategoryEnum::Table,
                                [
                                    CategoryEnum::Standard,
                                    CategoryEnum::Premium
                                ],
                            )
                            .not_null()
                            .default("standard"),
                    )
                    .col(
                        ColumnDef::new(Users::LearningLevel)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(Users::LearningStageLevel)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(ColumnDef::new(Users::CreatedAt).timestamp().null())
                    .col(ColumnDef::new(Users::UpdatedAt).timestamp().null())
                    .col(ColumnDef::new(Users::ExpiredAt).timestamp().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(ImageTransliterations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ImageTransliterations::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ImageTransliterations::IdUser)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ImageTransliterations::Image)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(ImageTransliterations::Title)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ImageTransliterations::Result)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ImageTransliterations::CreatedAt)
                            .timestamp()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-image-trans-user-id")
                            .from(ImageTransliterations::Table, ImageTransliterations::IdUser)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(TextTransliterations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(TextTransliterations::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(TextTransliterations::IdUser)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TextTransliterations::Input)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TextTransliterations::Result)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TextTransliterations::CreatedAt)
                            .timestamp()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-text-trans-user-id")
                            .from(TextTransliterations::Table, TextTransliterations::IdUser)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Transactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Transactions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Transactions::IdUser).uuid().not_null())
                    .col(ColumnDef::new(Transactions::IdMidtrans).uuid().not_null())
                    .col(
                        ColumnDef::new(Transactions::QrCode)
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Transactions::Title).string().not_null())
                    .col(ColumnDef::new(Transactions::Value).integer().not_null())
                    .col(
                        ColumnDef::new(Transactions::Status)
                            .enumeration(
                                StatusEnum::Table,
                                [
                                    StatusEnum::Success,
                                    StatusEnum::Pending,
                                    StatusEnum::Canceled,
                                ],
                            )
                            .not_null()
                            .default("pending"),
                    )
                    .col(ColumnDef::new(Transactions::CreatedAt).timestamp().null())
                    .col(ColumnDef::new(Transactions::UpdatedAt).timestamp().null())
                    .col(ColumnDef::new(Transactions::ExpiredAt).timestamp().null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-transactions-user-id")
                            .from(Transactions::Table, Transactions::IdUser)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Transactions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(TextTransliterations::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ImageTransliterations::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
    Email,
    PhotoProfile,
    FullName,
    Gender,
    DateOfBirth,
    Category,
    LearningLevel,
    LearningStageLevel,
    CreatedAt,
    UpdatedAt,
    ExpiredAt,
}

#[derive(Iden)]
enum ImageTransliterations {
    Table,
    Id,
    IdUser,
    Image,
    Title,
    Result,
    CreatedAt,
}

#[derive(Iden)]
enum TextTransliterations {
    Table,
    Id,
    IdUser,
    Input,
    Result,
    CreatedAt,
}

#[derive(Iden)]
enum Transactions {
    Table,
    Id,
    IdUser,
    IdMidtrans,
    QrCode,
    Title,
    Value,
    Status,
    CreatedAt,
    UpdatedAt,
    ExpiredAt,
}

#[derive(Iden)]
enum GenderEnum {
    Table,
    Male,
    Female,
}

#[derive(Iden)]
enum CategoryEnum {
    Table,
    Standard,
    Premium,
}

#[derive(Iden)]
enum StatusEnum {
    Table,
    Success,
    Pending,
    Canceled,
}
