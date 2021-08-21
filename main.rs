use actix_files::Files;
use actix_session::{CookieSession, Session};
use actix_web::{http, middleware, web, App, HttpRequest, HttpResponse, HttpServer, Result};
use askama::Template;
use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};

use data::get_distributor;
use sqlx::mysql::MySqlPool;
use sqlx::Done;
use sqlx::Row;
use std::env;
use std::str::FromStr;

use std::sync::Arc;

use argon2::{self, Config};

extern crate crypto;
extern crate rand;
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use rand::Rng;

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

#[macro_use]
extern crate lazy_static;

use serde::{Deserialize, Serialize};

use validator::{Validate, ValidationError};

use async_std::sync::Mutex;
use once_cell::sync::Lazy;

// Wrapper for the MySQLPool
#[derive(Clone)]
pub struct MySQL {
    conn: MySqlPool,
}

/* FORMS AND POST */
#[derive(Deserialize)]
struct OrderForm {
    firstname: String,
    lastname: String,
    tel: String,
    from_email: String,
    to_email: String,
    to_firstname: String,
    to_lastname: String,
    voucher: u64,
    amount: f64,
}

#[derive(Deserialize)]
struct PaymentHook {
    id: String,
}

#[derive(Deserialize)]
struct VoucherUpdateForm {
    hash: String,
    used: bool,
    balance: f64,
}

#[derive(Deserialize)]
struct AdminLoginForm {
    admin_username: String,
    admin_password: String,
}

#[derive(Deserialize)]
struct MijnZaakUpdateForm {
    description: String,
    email: String,
    tel: String,
    address: String,
    postalcode: String,
    city: String,
}

#[derive(Deserialize)]
struct AdminVouchersUpdateJson {
    three_option_vouchers: std::vec::Vec<u64>,
    price_range_voucher: PriceRangeJson,
    label_vouchers: std::vec::Vec<LabelVoucherJson>,
    days_valid: u64,
    one_use_only: bool,
}
#[derive(Deserialize)]
struct PriceRangeJson {
    min_amount: u64,
    max_amount: u64,
    auto_amount: u64,
}
#[derive(Deserialize)]
struct LabelVoucherJson {
    title: String,
    amount: u64,
    description: String,
}

/* TEMPLATES */

// Business templates

#[derive(Template)]
#[template(path = "index.html")]
struct Index<'a> {
    distributor: &'a Distributor,
    distributor_vouchers: &'a std::vec::Vec<DistributorVoucher>,
}

#[derive(Template)]
#[template(path = "bestel.html")]
struct Bestel<'a> {
    distributor_vouchers: &'a std::vec::Vec<DistributorVoucher>,
    first_distributor_voucher: &'a DistributorVoucher,
}

#[derive(Template)]
#[template(path = "bevestig.html")]
struct Bevestig {
    payment_url: String,
    voucher_price: String,
    total: String,
    from_str: String,
    to_str: String,
}

#[derive(Template)]
#[template(path = "faq.html")]
struct Faq;

#[derive(Template)]
#[template(path = "bon_mobile.html")]
struct VoucherPageMobile {
    number_code: String,
    distributor_name: String,
    balance: String,
    expiration_date: String,
    receiver_name: String,
}

#[derive(Template)]
#[template(path = "bon_desktop.html")]
struct VoucherPageDesktop {
    number_code: String,
    distributor_name: String,
    balance: String,
    expiration_date: String,
    receiver_name: String,
}

#[derive(Template)]
#[template(path = "success.html")]
struct Success {
    title: String,
    message: String,
}

#[derive(Template)]
#[template(path = "niet-gelukt.html")]
struct Failed;

#[derive(Template)]
#[template(path = "scanner/scanner.html")]
struct Scanner;

#[derive(Template)]
#[template(path = "scanner/login.html")]
struct ScannerLogin;

#[derive(Template)]
#[template(path = "404.html")]
struct Error404;

// Administrator templates

#[derive(Template)]
#[template(path = "admin/login.html")]
struct AdminLogin {
    distributor_name: String,
    login_status: String,
}

#[derive(Template)]
#[template(path = "admin/mijn-zaak.html")]
struct AdminDashboardMijnZaak {
    distributor: Distributor,
}

#[derive(Template)]
#[template(path = "admin/cadeaubonnen.html")]
struct AdminDashboardCadeaubonnen<'a> {
    // Three price vouchers
    three_price_voucher_a: f64,
    three_price_voucher_b: f64,
    three_price_voucher_c: f64,
    three_price_voucher_max_days: u16,
    three_price_voucher_one_use_only: bool,

    // Price range vouchers
    price_range_voucher_min: f64,
    price_range_voucher_max: f64,
    price_range_voucher_auto: f64,
    price_range_voucher_max_days: u16,
    price_range_voucher_one_use_only: bool,

    // Label vouchers
    label_vouchers: &'a Vec<LabelVoucherData>,
    label_voucher_max_days: u16,

    // Currently active voucher
    currently_active: Option<VoucherType>,
}

struct LabelVoucherData {
    title: String,
    amount: u64,
    description: String,
    days_valid: u16,
}
impl Default for LabelVoucherData {
    fn default() -> Self {
        LabelVoucherData {
            title: "".to_string(),
            amount: 10 as u64,
            description: "".to_string(),
            days_valid: 90,
        }
    }
}

#[derive(Template)]
#[template(path = "admin/bestellingen.html")]
struct AdminDashboardBestellingen;

#[derive(Deserialize, Serialize)]
struct AdminOrderTableData {
    id: u64,
    purchase_amount: f64,
    payment_status: i16,
    purchase_date: String,
    client_name: String,
}

#[derive(Deserialize, Serialize, Template)]
#[template(path = "admin/bestelling.html")]
struct AdminOrderData {
    voucher: Voucher,
}

#[derive(Deserialize, Debug)]
pub struct AdminOrderFilterParams {
    amount: u64,
    search_query: Option<String>,
    min_amount: Option<f64>,
    max_amount: Option<f64>,
    min_date: Option<String>,
    max_date: Option<String>,
    statusses: Option<String>,
}

#[derive(Template)]
#[template(path = "admin/wachtwoord.html")]
struct AdminDashboardWachtwoord;

#[derive(Template)]
#[template(path = "admin/help.html")]
struct AdminDashboardHelp;

#[derive(Deserialize, Serialize, PartialEq, Debug, FromPrimitive)]
enum OrderStatus {
    Failed = 0,
    Paid = 1,
}

/* OBJECTS */

// Business OBJECTS

#[derive(Serialize, Deserialize, Clone)]
pub struct Distributor {
    id: u64,
    name: String,
    email: String,
    tel: String,
    address: String,
    location: Location,
    subdomain: String,
    description: String,
    //#[serde(skip_serializing)]
    bankaccountnr: String,
    btw_nr: String,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct Client {
    id: u64,
    firstname: String,
    lastname: String,
    email: String,
    tel: String,
    saved_account: bool,
}
#[derive(Deserialize, Serialize, Clone)]
pub struct Sale {
    id: u64,
    client: Client,
    amount: f64,
    payment_id: String,
    paid: bool,
    purchase_date: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Deserialize, Serialize)]
pub struct Voucher {
    id: u64,
    sale: Sale,
    receiver_email: String,
    receiver_name: String,
    distributorvoucher: DistributorVoucher,
    balance: f64,
    used: bool,
    #[serde(skip_serializing)]
    expiration_date: chrono::DateTime<chrono::Utc>,
    hash_code: String,
    number_code: String,
    version: i64,
}

#[derive(Deserialize, Serialize, PartialEq, Debug)]
enum VoucherType {
    ThreeOptionVoucher,
    RangeVoucher,
    LabelVoucher,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Location {
    id: u64,
    postalcode: String,
    city: String,
}

#[derive(Deserialize, Serialize)]
pub struct DistributorVoucher {
    id: u64,
    distributor: Distributor,
    voucher_type: VoucherType,
    amount: f64,
    min_amount: f64,
    max_amount: f64,
    label: String,
    description: String,
    days_valid: u16,
    active: bool,
    one_use_only: bool,
    create_date: Option<chrono::DateTime<chrono::Utc>>,
}

// Administrator objects
#[derive(Serialize, Deserialize, Clone)]
pub struct DistributorUser {
    id: u64,
    //#[serde(skip_serializing)]
    username: String,
    //#[serde(skip_serializing)]
    password: String,
    distributor: Distributor,
    display_name: String,
}

impl DistributorUser {
    pub async fn create(user: &mut DistributorUser, mysql: &web::Data<MySQL>) -> u64 {
        user.hash_password();

        let result = sqlx::query("INSERT INTO distributoruser (username, password, distributor, display_name) VALUES (?,?,?,?)")
        .bind(&user.username)
        .bind(&user.password)
        .bind(&user.distributor.id)
        .bind(&user.display_name)
        .execute(&mysql.conn).await;

        match result {
            Err(e) => {
                println!("Error: {}", e);
                0
            }
            Ok(r) => r.last_insert_id(),
        }
    }

    pub fn hash_password(&mut self) -> Result<(), argon2::Error> {
        let salt: [u8; 32] = rand::thread_rng().gen();
        let config = Config::default();

        self.password = argon2::hash_encoded(self.password.as_bytes(), &salt, &config)
            .map_err(|e| argon2::Error::from(e))
            .unwrap();

        Ok(())
    }

    pub fn verify_password(&self, password: &[u8]) -> Result<bool, argon2::Error> {
        argon2::verify_encoded(&self.password, password).map_err(|e| argon2::Error::from(e))
    }

    pub async fn get_by_username(
        username: &String,
        mysql: &web::Data<MySQL>,
    ) -> Option<DistributorUser> {
        let mut result = sqlx::query("SELECT ID, username, password, distributor, display_name FROM distributoruser WHERE username = ?")
        .bind(&username)
        .fetch_one(&mysql.conn).await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(DistributorUser {
                id: r.try_get("ID").unwrap(),
                username: r.try_get("username").unwrap(),
                password: r.try_get("password").unwrap(),
                distributor: get_distributor(mysql, r.try_get("distributor").unwrap())
                    .await
                    .unwrap(),
                display_name: r.try_get("display_name").unwrap(),
            }),
        }
    }
}

impl FromStr for VoucherType {
    type Err = ();

    fn from_str(input: &str) -> Result<VoucherType, Self::Err> {
        match &*input.to_lowercase() {
            "threeoptionvoucher" => Ok(VoucherType::ThreeOptionVoucher),
            "rangevoucher" => Ok(VoucherType::RangeVoucher),
            "labelvoucher" => Ok(VoucherType::LabelVoucher),
            _ => Err(()),
        }
    }
}
impl std::fmt::Display for VoucherType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for OrderStatus {
    type Err = ();

    fn from_str(input: &str) -> Result<OrderStatus, Self::Err> {
        match &*input.to_lowercase() {
            "failed" => Ok(OrderStatus::Failed),
            "paid" => Ok(OrderStatus::Paid),
            _ => Err(()),
        }
    }
}
impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

fn get_subdomain_part(full_domain: &str) -> &str {
    full_domain.split(".").next().unwrap()
}

pub mod data {
    use crate::data::Selector::*;
    use crate::*;
    use futures::TryStreamExt;

    pub enum Selector {
        ById(u64),
        ByPaymentId(String),
        ByHash(String),
        ByNumberCode(String),
    }

    pub async fn get_distributor_by_subdomain(
        mysql: &web::Data<MySQL>,
        subdomain: &str,
    ) -> Option<Distributor> {
        let mut result = sqlx::query("SELECT ID, name, email, tel, address, location, description, bankaccountnr, btw_nr FROM distributor WHERE subdomain = ?")
        .bind(&subdomain)
        .fetch_one(&mysql.conn).await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(Distributor {
                id: r.try_get("ID").unwrap(),
                name: r.try_get("name").unwrap(),
                email: r.try_get("email").unwrap(),
                tel: r.try_get("tel").unwrap(),
                address: r.try_get("address").unwrap(),
                location: get_location(mysql, r.try_get("location").unwrap())
                    .await
                    .unwrap(),
                subdomain: subdomain.to_string(),
                description: r.try_get("description").unwrap(),
                bankaccountnr: r.try_get("bankaccountnr").unwrap(),
                btw_nr: r.try_get("btw_nr").unwrap(),
            }),
        }
    }

    pub async fn get_distributor(mysql: &web::Data<MySQL>, id: u64) -> Option<Distributor> {
        let mut result = sqlx::query("SELECT name, email, tel, address, location, subdomain, description, bankaccountnr, btw_nr FROM distributor WHERE ID = ?")
        .bind(&id)
        .fetch_one(&mysql.conn).await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(Distributor {
                id: id,
                name: r.try_get("name").unwrap(),
                email: r.try_get("email").unwrap(),
                tel: r.try_get("tel").unwrap(),
                address: r.try_get("address").unwrap(),
                location: get_location(mysql, r.try_get("location").unwrap())
                    .await
                    .unwrap(),
                subdomain: r.try_get("subdomain").unwrap(),
                description: r.try_get("description").unwrap(),
                bankaccountnr: r.try_get("bankaccountnr").unwrap(),
                btw_nr: r.try_get("btw_nr").unwrap(),
            }),
        }
    }

    pub async fn update_distributor(mysql: &web::Data<MySQL>, distributor: &Distributor) -> bool {
        let location = get_id_of_location(mysql, &distributor.location).await;
        if location.is_none() {
            return false;
        }

        let result = sqlx::query("UPDATE distributor SET name=?, email=?, tel=?, address=?, location=?, subdomain=?, description=?, bankaccountnr=?, btw_nr=? WHERE ID = ?")
        .bind(&distributor.name)
        .bind(&distributor.email)
        .bind(&distributor.tel)
        .bind(&distributor.address)
        .bind(&location.unwrap())
        .bind(&distributor.subdomain)
        .bind(&distributor.description)
        .bind(&distributor.bankaccountnr)
        .bind(&distributor.btw_nr)
        .bind(&distributor.id)
        .execute(&mysql.conn).await;

        match result {
            Err(e) => {
                println!("Error: {}", e);
                false
            }
            Ok(r) => r.rows_affected() > 0,
        }
    }

    pub async fn add_client(mysql: &web::Data<MySQL>, client: &Client) -> u64 {
        let result =
            sqlx::query("INSERT INTO client (firstname, lastname, email, tel) VALUES (?,?,?,?)")
                .bind(&client.firstname)
                .bind(&client.lastname)
                .bind(&client.email)
                .bind(&client.tel)
                .execute(&mysql.conn)
                .await;

        match result {
            Err(e) => {
                println!("Error: {}", e);
                0
            }
            Ok(r) => r.last_insert_id(),
        }
    }

    pub async fn get_client(mysql: &web::Data<MySQL>, id: u64) -> Option<Client> {
        let mut result = sqlx::query(
            "SELECT firstname, lastname, email, tel, saved_account FROM client WHERE ID = ?",
        )
        .bind(&id)
        .fetch_one(&mysql.conn)
        .await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(Client {
                id: id,
                firstname: r.try_get("firstname").unwrap(),
                lastname: r.try_get("lastname").unwrap(),
                email: r.try_get("email").unwrap(),
                tel: r.try_get("tel").unwrap(),
                saved_account: r.try_get("saved_account").unwrap(),
            }),
        }
    }

    pub async fn add_sale(mysql: &web::Data<MySQL>, sale: &Sale) -> u64 {
        let result = sqlx::query("INSERT INTO sale (client, amount, payment_id) VALUES (?,?,?)")
            .bind(&sale.client.id)
            .bind(&sale.amount)
            .bind(&sale.payment_id)
            .execute(&mysql.conn)
            .await;

        match result {
            Err(e) => {
                println!("Error: {}", e);
                0
            }
            Ok(r) => r.last_insert_id(),
        }
    }

    pub async fn get_sale(mysql: &web::Data<MySQL>, selector: Selector) -> Option<Sale> {
        let (where_column, where_value) = match selector {
            ById(id) => ("ID", id.to_string()),
            ByPaymentId(payment_id) => ("payment_id", payment_id),
            _ => ("", "".to_string()),
        };

        let sql = format!(
            "SELECT ID, client, amount, payment_id, paid, purchase_date FROM sale WHERE {} = ?",
            where_column
        );
        let mut result = sqlx::query(&sql)
            .bind(&where_value)
            .fetch_one(&mysql.conn)
            .await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(Sale {
                id: r.try_get("ID").unwrap(),
                client: get_client(&mysql, r.try_get("client").unwrap())
                    .await
                    .unwrap(),
                amount: r.try_get("amount").unwrap(),
                payment_id: r.try_get("payment_id").unwrap(),
                paid: r.try_get("paid").unwrap(),
                purchase_date: r.try_get("purchase_date").unwrap(),
            }),
        }
    }

    pub async fn update_sale(mysql: &web::Data<MySQL>, sale: &Sale, selector: Selector) -> bool {
        let (where_column, where_value) = match selector {
            ById(id) => ("ID", id.to_string()),
            ByPaymentId(payment_id) => ("payment_id", payment_id),
            _ => ("", "".to_string()),
        };

        let sql = format!(
            "UPDATE sale SET client=?, amount=?, payment_id=?, paid=? WHERE {} = ?",
            where_column
        );

        let mut result = sqlx::query(&sql)
            .bind(&sale.client.id)
            .bind(&sale.amount)
            .bind(&sale.payment_id)
            .bind(&sale.paid)
            .bind(&where_value)
            .execute(&mysql.conn)
            .await;

        match result {
            Err(e) => {
                println!("Error: {}", e);
                false
            }
            Ok(r) => r.rows_affected() > 0,
        }
    }

    pub async fn get_distributor_voucher(
        mysql: &web::Data<MySQL>,
        id: u64,
    ) -> Option<DistributorVoucher> {
        let mut result = sqlx::query("SELECT distributor, voucher_type, amount, min_amount, max_amount, label, description, days_valid, active, one_use_only, create_date FROM distributorvoucher WHERE ID = ?")
        .bind(&id)
        .fetch_one(&mysql.conn).await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(DistributorVoucher {
                id: id,
                distributor: get_distributor(&mysql, r.try_get("distributor").unwrap())
                    .await
                    .unwrap(),
                voucher_type: VoucherType::from_str(r.try_get("voucher_type").unwrap()).unwrap(),
                amount: r.try_get("amount").unwrap(),
                min_amount: r.try_get("min_amount").unwrap(),
                max_amount: r.try_get("max_amount").unwrap(),
                label: match r.try_get("label").unwrap() {
                    None => "".to_string(),
                    Some(v) => v,
                },
                description: match r.try_get("description").unwrap() {
                    None => "".to_string(),
                    Some(v) => v,
                },
                days_valid: r.try_get("days_valid").unwrap(),
                active: r.try_get("active").unwrap(),
                one_use_only: r.try_get("one_use_only").unwrap(),
                create_date: r.try_get("create_date").unwrap(),
            }),
        }
    }

    pub async fn get_distributor_vouchers_by_distributor(
        mysql: &web::Data<MySQL>,
        id: u64,
    ) -> Option<std::vec::Vec<DistributorVoucher>> {
        let mut result = sqlx::query(
            "SELECT ID FROM distributorvoucher WHERE distributor = ? AND most_recent_of_Type=1",
        )
        .bind(&id)
        .fetch(&mysql.conn);

        let mut distributor_vouchers: std::vec::Vec<DistributorVoucher> = std::vec::Vec::new();

        while let Some(row) = result.try_next().await.unwrap() {
            distributor_vouchers.push(
                get_distributor_voucher(&mysql, row.try_get("ID").unwrap())
                    .await
                    .unwrap(),
            );
        }

        Some(distributor_vouchers)
    }
    pub async fn get_active_distributor_vouchers_by_distributor(
        mysql: &web::Data<MySQL>,
        id: u64,
    ) -> Option<std::vec::Vec<DistributorVoucher>> {
        let mut result =
            sqlx::query("SELECT ID FROM distributorvoucher WHERE distributor = ? AND active=1")
                .bind(&id)
                .fetch(&mysql.conn);

        let mut distributor_vouchers: std::vec::Vec<DistributorVoucher> = std::vec::Vec::new();

        while let Some(row) = result.try_next().await.unwrap() {
            distributor_vouchers.push(
                get_distributor_voucher(&mysql, row.try_get("ID").unwrap())
                    .await
                    .unwrap(),
            );
        }

        Some(distributor_vouchers)
    }
    pub async fn add_active_distributor_vouchers(
        mysql: &web::Data<MySQL>,
        distributor_vouchers: std::vec::Vec<DistributorVoucher>,
    ) -> bool {
        if distributor_vouchers.len() == 0 {
            return false;
        }

        let mut result = sqlx::query("UPDATE distributorvoucher SET most_recent_of_type=0 WHERE distributor=? AND upper(voucher_type)=upper(?)")
        .bind(distributor_vouchers[0].distributor.id)
        .bind(distributor_vouchers[0].voucher_type.to_string())
        .execute(&mysql.conn).await;

        let mut result = sqlx::query("UPDATE distributorvoucher SET active=0 WHERE distributor=?")
            .bind(distributor_vouchers[0].distributor.id)
            .execute(&mysql.conn)
            .await;

        for distributor_voucher in distributor_vouchers {
            let mut result = sqlx::query("INSERT INTO distributorvoucher (distributor, voucher_type, amount, min_amount, max_amount, label, description, days_valid, active, one_use_only, most_recent_of_type) VALUES (?,?,?,?,?,?,?,?,?,?,1)")
            .bind(distributor_voucher.distributor.id)
            .bind(distributor_voucher.voucher_type.to_string())
            .bind(distributor_voucher.amount)
            .bind(distributor_voucher.min_amount)
            .bind(distributor_voucher.max_amount)
            .bind(distributor_voucher.label.to_string())
            .bind(distributor_voucher.description.to_string())
            .bind(distributor_voucher.days_valid)
            .bind(distributor_voucher.active)
            .bind(distributor_voucher.one_use_only)
            .execute(&mysql.conn).await;
        }

        true
    }

    pub async fn get_distributor_vouchers_by_sale(
        mysql: &web::Data<MySQL>,
        id: u64,
    ) -> Option<std::vec::Vec<DistributorVoucher>> {
        let sql = format!("SELECT distributorvoucher FROM voucher WHERE sale = ?");
        let mut result = sqlx::query(&sql).bind(&id).fetch(&mysql.conn);

        let mut distributor_vouchers: std::vec::Vec<DistributorVoucher> = std::vec::Vec::new();

        while let Some(row) = result.try_next().await.unwrap() {
            distributor_vouchers.push(
                get_distributor_voucher(&mysql, row.try_get("distributorvoucher").unwrap())
                    .await
                    .unwrap(),
            );
        }

        Some(distributor_vouchers)
    }

    pub async fn get_voucher(mysql: &web::Data<MySQL>, selector: Selector) -> Option<Voucher> {
        let (where_column, where_value) = match selector {
            ById(id) => ("ID", id.to_string()),
            ByHash(hash) => ("hash_code", hash),
            ByNumberCode(number_code) => ("number_code", number_code),
            _ => ("", "".to_string()),
        };

        let sql = format!("SELECT ID, sale, receiver_email, receiver_name, distributorvoucher, balance, used, expiration_date, hash_code, number_code, version FROM voucher WHERE {} = ?", where_column);
        let mut result = sqlx::query(&sql)
            .bind(&where_value)
            .fetch_one(&mysql.conn)
            .await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(Voucher {
                id: r.try_get("ID").unwrap(),
                sale: data::get_sale(&mysql, data::Selector::ById(r.try_get("sale").unwrap()))
                    .await
                    .unwrap(),
                receiver_email: r.try_get("receiver_email").unwrap(),
                receiver_name: r.try_get("receiver_name").unwrap(),
                distributorvoucher: get_distributor_voucher(
                    &mysql,
                    r.try_get("distributorvoucher").unwrap(),
                )
                .await
                .unwrap(),
                balance: r.try_get("balance").unwrap(),
                used: r.try_get("used").unwrap(),
                expiration_date: r.try_get("expiration_date").unwrap(),
                hash_code: r.try_get("hash_code").unwrap(),
                number_code: r.try_get("number_code").unwrap(),
                version: r.try_get("version").unwrap(),
            }),
        }
    }

    pub async fn add_voucher(mysql: &web::Data<MySQL>, voucher: &Voucher) -> u64 {
        let result = sqlx::query("INSERT INTO voucher (sale, receiver_email, receiver_name, distributorvoucher, balance, used, expiration_date, hash_code, number_code, version) VALUES (?,?,?,?,?,?,?,?,?,?)")
        .bind(&voucher.sale.id)
        .bind(&voucher.receiver_email)
        .bind(&voucher.receiver_name)
        .bind(&voucher.distributorvoucher.id)
        .bind(&voucher.balance)
        .bind(&voucher.used)
        .bind(&voucher.expiration_date)
        .bind(&voucher.hash_code)
        .bind(&voucher.number_code)
        .bind(&voucher.version)
        .execute(&mysql.conn).await;

        match result {
            Err(e) => {
                println!("Error: {}", e);
                0
            }
            Ok(r) => r.last_insert_id(),
        }
    }

    pub async fn update_voucher(mysql: &web::Data<MySQL>, voucher: &Voucher) -> bool {
        let sql = format!("UPDATE voucher SET balance=?, used=? WHERE ID = ?");

        let mut result = sqlx::query(&sql)
            .bind(&voucher.balance)
            .bind(&voucher.used)
            .bind(&voucher.id)
            .execute(&mysql.conn)
            .await;

        match result {
            Err(e) => {
                println!("Error: {}", e);
                false
            }
            Ok(r) => r.rows_affected() > 0,
        }
    }

    pub async fn get_location(mysql: &web::Data<MySQL>, id: u64) -> Option<Location> {
        let mut result = sqlx::query("SELECT postalcode, city FROM location WHERE ID = ?")
            .bind(&id)
            .fetch_one(&mysql.conn)
            .await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(Location {
                id: id,
                postalcode: r.try_get("postalcode").unwrap(),
                city: r.try_get("city").unwrap(),
            }),
        }
    }

    pub async fn get_id_of_location(mysql: &web::Data<MySQL>, location: &Location) -> Option<u64> {
        let mut result = sqlx::query("SELECT ID FROM location WHERE postalcode = ? AND city = ?")
            .bind(&location.postalcode)
            .bind(&location.city)
            .fetch_one(&mysql.conn)
            .await;

        match result {
            Err(e) => {
                println!("error: {:?}", e);
                None
            }
            Ok(r) => Some(r.try_get("ID").unwrap()),
        }
    }

    pub async fn get_all_orders(
        mysql: &web::Data<MySQL>,
        start: u64,
        amount: u64,
        mut filters: AdminOrderFilterParams,
        distributor_id: u64,
    ) -> Vec<AdminOrderTableData> {
        let mut statusses: std::vec::Vec<u16> = std::vec::Vec::new();

        let mut where_str: String = "".to_string();
        // Needed when filter for date is PREV_HOUR or MOST_RECENT_ACTIVATION
        let current_date = chrono::offset::Utc::now();
        let mut cmp_min_date: Option<String> = None;
        let mut cmp_max_date: Option<String> = None;

        if filters.min_amount.is_some() {
            where_str = format!("{}{}", where_str, " (sale.amount >= ?) ");
        } else {
            where_str = format!("{}{}", where_str, " (? IS NULL) ");
        }
        if filters.max_amount.is_some() {
            where_str = format!("{}{}", where_str, " AND (sale.amount < ?) ");
        } else {
            where_str = format!("{}{}", where_str, " AND (? IS NULL) ");
        }
        if filters.min_date.is_some() {
            where_str = format!("{}{}", where_str, " AND (sale.purchase_date >= ?) ");

            match filters.min_date.as_deref() {
                Some("PREV_HOUR") => {
                    cmp_min_date = Some(
                        (current_date - chrono::Duration::hours(1))
                            .format("%Y-%m-%d %H:%M")
                            .to_string(),
                    );
                }
                Some("MOST_RECENT_ACTIVATION") => {
                    cmp_min_date = Some(
                        get_active_distributor_vouchers_by_distributor(mysql, distributor_id)
                            .await
                            .unwrap()[0]
                            .create_date
                            .unwrap()
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string(),
                    );
                }
                _ => {
                    cmp_min_date = Some(format!("{} 00:00:00", filters.min_date.unwrap()));
                }
            }
        } else {
            where_str = format!("{}{}", where_str, " AND (? IS NULL) ");
        }
        if filters.max_date.is_some() {
            where_str = format!("{}{}", where_str, " AND (sale.purchase_date <= ?) ");

            match filters.max_date.as_deref() {
                Some("PREV_HOUR") => {
                    cmp_max_date = Some(
                        (current_date + chrono::Duration::seconds(120))
                            .format("%Y-%m-%d %H:%M")
                            .to_string(),
                    );
                }
                Some("MOST_RECENT_ACTIVATION") => {
                    cmp_max_date = Some("3000-01-01 23:59:59".to_string());
                }
                _ => {
                    cmp_max_date = Some(format!("{} 23:59:59", filters.max_date.unwrap()));
                }
            }
        } else {
            where_str = format!("{}{}", where_str, " AND (? IS NULL) ");
        }
        if filters.search_query.is_some() {
            where_str = format!(
                "{}{}",
                where_str, " AND UPPER(CONCAT(client.firstname, ' ', client.lastname, '$', client.email, '$', client.tel, '$', voucher.number_code, '$', voucher.receiver_name, '$', voucher.receiver_email, '$', DATE_FORMAT(DATE(CONVERT_TZ(sale.purchase_date, '+00:00', '+01:00')), '%d-%m-%Y'), '$', sale.payment_id)) LIKE ? "
            );
            filters.search_query = Some(format!(
                "%{}%",
                filters.search_query.unwrap().to_uppercase()
            ));
        } else {
            where_str = format!("{}{}", where_str, " AND (? IS NULL) ");
        }

        if filters.statusses.is_some() {
            statusses = filters
                .statusses
                .unwrap()
                .split(",")
                .map(|x| x.parse::<u16>().unwrap())
                .collect();

            let mut in_str: String = "".to_string();
            for i in &statusses {
                in_str = format!("{}?,", in_str);
            }
            in_str.pop();

            where_str = format!("{} AND sale.paid IN ({}) ", where_str, in_str);
        } else {
            where_str = format!("{}{}", where_str, " AND 1=1 ");
        }

        if where_str.chars().count() > 0 {
            where_str = format!("{}{}", " WHERE ", where_str);
        }

        let sql = format!("SELECT voucher.ID, sale.amount, sale.paid, sale.purchase_date, CONCAT(client.firstname, ' ', client.lastname) as client_name FROM voucher INNER JOIN sale ON sale.ID=voucher.sale INNER JOIN client ON client.ID=sale.client {} ORDER BY sale.purchase_date DESC, voucher.id DESC LIMIT ? OFFSET ?", where_str);

        let mut query = sqlx::query(&sql)
            .bind(filters.min_amount)
            .bind(filters.max_amount)
            .bind(cmp_min_date)
            .bind(cmp_max_date)
            .bind(filters.search_query);

        for status in statusses {
            query = query.bind(status);
        }

        let mut result = query.bind(amount).bind(start).fetch_all(&mysql.conn).await;

        let mut order_data: std::vec::Vec<AdminOrderTableData> = std::vec::Vec::new();

        for r in result.unwrap().iter() {
            let date: chrono::DateTime<chrono::Utc> = r.try_get("purchase_date").unwrap();

            order_data.push(AdminOrderTableData {
                id: r.try_get("ID").unwrap(),
                purchase_amount: r.try_get("amount").unwrap(),
                payment_status: r.try_get("paid").unwrap(),
                purchase_date: date
                    .with_timezone(&chrono::Local)
                    .format("%d-%m-%Y %H:%M")
                    .to_string(),
                client_name: r.try_get("client_name").unwrap(),
            });
        }

        order_data
    }
}

pub mod mollie {
    use crate::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use serde_json::value;

    #[derive(Clone)]
    pub struct Mollie {
        pub api_key: String,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Amount {
        currency: String,
        value: String,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Link {
        pub href: String,
        r#type: String,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Payment {
        #[serde(skip_serializing)]
        id: String,
        description: String,
        amount: Amount,
        redirectUrl: String,
        webhookUrl: String,
        #[serde(rename = "_links")]
        pub links: HashMap<String, Link>,
        #[serde(skip_serializing)]
        pub status: String,
    }

    impl Mollie {
        pub async fn make_payment(&self, sale: &Sale, payment_id: &mut String) -> Payment {
            let mut payment = Payment {
                id: "".to_string(),
                description: format!("My Description for {}", sale.client.email),
                amount: Amount {
                    currency: "EUR".to_string(),
                    value: format!("{:.2}", sale.amount + 1.5),
                },
                redirectUrl: "http://demok.kaddo.test:8080".to_string(),
                webhookUrl:
                    "https://b27b-2a02-1810-3807-8a00-2453-bf80-2799-ff4b.ngrok.io/payment/hook"
                        .to_string(),
                status: "".to_string(),
                links: HashMap::new(),
            };

            let http = reqwest::Client::new();
            let res = http
                .post("https://api.mollie.com/v2/payments")
                .json(&payment)
                .header(
                    "Authorization",
                    "Bearer test_8v3dnuvdFp7hj6U9TCvtCpr8MsNeHx",
                )
                .send()
                .await
                .unwrap();

            let json: serde_json::Value = serde_json::from_str(&res.text().await.unwrap()).unwrap();

            *payment_id = str::replace(&json["id"].to_string(), "\"", "");

            payment.redirectUrl = format!("http://demok.kaddo.test:8080/check/{}", payment_id);

            payment.id = payment_id.to_string();
            self.update_payment(&payment).await;

            payment
        }

        pub async fn update_payment(&self, payment: &Payment) -> bool {
            #[derive(Serialize)]
            struct Parameters {
                redirectUrl: String,
            }

            let params = Parameters {
                redirectUrl: (*payment.redirectUrl).to_string(),
            };

            let http = reqwest::Client::new();
            let res = http
                .patch(&format!(
                    "https://api.mollie.com/v2/payments/{}",
                    payment.id
                ))
                .json(&params)
                .header(
                    "Authorization",
                    "Bearer test_8v3dnuvdFp7hj6U9TCvtCpr8MsNeHx",
                )
                .send()
                .await
                .unwrap();

            true
        }

        pub async fn get_payment(&self, id: &String) -> Payment {
            let http = reqwest::Client::new();
            let res = http
                .patch(&format!("https://api.mollie.com/v2/payments/{}", id))
                .header(
                    "Authorization",
                    "Bearer test_8v3dnuvdFp7hj6U9TCvtCpr8MsNeHx",
                )
                .send()
                .await
                .unwrap();

            let payment: Payment = serde_json::from_str(&res.text().await.unwrap()).unwrap();

            payment
        }
    }
}

pub mod mail {
    use crate::*;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};
    use lettre_email::EmailBuilder;

    #[derive(Clone)]
    pub struct Creds {
        pub username: String,
        pub password: String,
    }

    #[derive(Clone)]
    pub struct Mail {
        pub creds: Creds,
        pub smtp_server: String,
    }

    impl Mail {
        pub async fn send_plain_mail(
            &self,
            from: String,
            to: String,
            subject: String,
            text: String,
        ) {
            let email = Message::builder()
                .from(from.parse().unwrap())
                .reply_to(to.parse().unwrap())
                .to(to.parse().unwrap())
                .subject(subject)
                .body(text)
                .unwrap();

            let mailer = SmtpTransport::unencrypted_localhost();

            match mailer.send(&email) {
                Ok(_) => println!("Email sent successfully!"),
                Err(e) => panic!("Could not send email: {:?}", e),
            }
        }
    }
}

/* ROUTE FUNCTIONS */

// Business routes

async fn index(mysql: web::Data<MySQL>, req: HttpRequest) -> Result<HttpResponse> {
    let distributor = data::get_distributor_by_subdomain(
        &mysql,
        get_subdomain_part(req.headers().get("Host").unwrap().to_str().unwrap()),
    )
    .await;

    if distributor.is_none() {
        return error404().await;
    }

    let d = distributor.unwrap();

    let s = Index {
        distributor: &d,
        distributor_vouchers: &data::get_active_distributor_vouchers_by_distributor(&mysql, d.id)
            .await
            .unwrap(),
    }
    .render()
    .unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn bestel(mysql: web::Data<MySQL>, req: HttpRequest) -> Result<HttpResponse> {
    let distributor = data::get_distributor_by_subdomain(
        &mysql,
        get_subdomain_part(req.headers().get("Host").unwrap().to_str().unwrap()),
    )
    .await;

    if distributor.is_none() {
        return error404().await;
    }

    let d = distributor.unwrap();

    let distributor_vouchers = &data::get_active_distributor_vouchers_by_distributor(&mysql, d.id)
        .await
        .unwrap();
    let s = Bestel {
        distributor_vouchers: &distributor_vouchers,
        first_distributor_voucher: &distributor_vouchers[0],
    }
    .render()
    .unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn faq() -> Result<HttpResponse> {
    let s = Faq.render().unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn order_form(
    mysql: web::Data<MySQL>,
    mut form: web::Form<OrderForm>,
    req: HttpRequest,
    mollie: web::Data<mollie::Mollie>,
) -> String {
    // Make client of this voucher
    let mut client = Client {
        id: 0,
        firstname: form.firstname.to_string(),
        lastname: form.lastname.to_string(),
        email: form.from_email.to_string(),
        tel: form.tel.to_string(),
        saved_account: false,
    };

    // Get distributor of this subpage and the voucher being purchased
    let distributor = data::get_distributor_by_subdomain(
        &mysql,
        get_subdomain_part(req.headers().get("Host").unwrap().to_str().unwrap()),
    )
    .await
    .unwrap();
    let distributor_voucher = data::get_distributor_voucher(&mysql, form.voucher)
        .await
        .unwrap();

    // Add client to database
    let client_id = data::add_client(&mysql, &client).await;
    client.id = client_id;

    // Check if amount is not altered on the client-side
    if distributor_voucher.voucher_type != VoucherType::RangeVoucher
        && form.amount != distributor_voucher.amount
    {
        return "/niet-gelukt".to_string();
    }

    // Make Sale object with default values
    let amount: f64 = form.amount;
    let mut sale = Sale {
        id: 0,
        client: client,
        amount: amount,
        payment_id: "".to_string(),
        paid: false,
        purchase_date: None,
    };

    // Add sale to database and get ID of sale
    let sale_id = data::add_sale(&mysql, &sale).await;
    sale.id = sale_id;

    // Generate voucher being purchased
    let expiration_date = Utc::now()
        .with_timezone(&chrono::Local)
        .with_timezone(&chrono::Utc)
        + Duration::days(distributor_voucher.days_valid.into());
    let random_number: u16 = rand::thread_rng().gen();
    let random_number_str = format!("{}-{}", client_id, random_number);

    let mut sha256 = Sha256::new();
    sha256.input_str(&random_number_str);
    let hash = &sha256.result_str();

    if form.to_email == "" {
        form.to_email = form.from_email.to_string();
    }
    if form.to_firstname == "" {
        form.to_firstname = form.firstname.to_string();
    }
    if form.to_lastname == "" {
        form.to_lastname = form.lastname.to_string();
    }

    let mut voucher = Voucher {
        id: 0,
        sale: sale.clone(),
        receiver_email: form.to_email.to_string(),
        receiver_name: format!("{} {}", form.to_firstname, form.to_lastname),
        balance: form.amount,
        distributorvoucher: distributor_voucher,
        used: false,
        expiration_date: expiration_date,
        hash_code: hash.to_string(),
        number_code: random_number_str,
        version: 1,
    };

    // Add voucher to database
    data::add_voucher(&mysql, &voucher).await;

    // Make payment and retrieve payment_id
    let mut payment_id = "".to_string(); // payment_id is passed by reference and gets a new value
    mollie.make_payment(&sale, &mut payment_id).await;
    sale.payment_id = payment_id;

    // Update sale
    data::update_sale(&mysql, &sale, data::Selector::ById(sale_id)).await;

    format!("/bevestig/{}", hash)
}

async fn payment_hook(
    mollie: web::Data<mollie::Mollie>,
    mysql: web::Data<MySQL>,
    mail: web::Data<mail::Mail>,
    data: web::Form<PaymentHook>,
) -> Result<HttpResponse> {
    let payment = mollie.get_payment(&data.id).await;

    if payment.status == "paid" {
        let mut sale = data::get_sale(&mysql, data::Selector::ByPaymentId(data.id.clone()))
            .await
            .unwrap();

        sale.paid = true;
        data::update_sale(&mysql, &sale, data::Selector::ByPaymentId(data.id.clone())).await;

        mail.send_plain_mail(
            "Kaddo. <noreply@kaddo.be>".to_string(),
            format!(
                "{} {} <{}>",
                sale.client.firstname, sale.client.lastname, sale.client.email
            ),
            "Betaling ontvangen!".to_string(),
            "Rofl".to_string(),
        )
        .await;

        println!("Paid, updating ID {}", data.id.clone());
        println!("{}", serde_json::to_string(&sale).unwrap());

        return Ok(HttpResponse::Ok().finish());
    } else {
        return Ok(HttpResponse::Ok().finish());
    }
}

async fn check(
    mollie: web::Data<mollie::Mollie>,
    web::Path(payment_id): web::Path<(String)>,
) -> Result<HttpResponse> {
    let payment = mollie.get_payment(&payment_id).await;

    match payment.status == "paid" {
        true => Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/succes/aankoop")
            .finish()),
        _ => Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/niet-gelukt")
            .finish()),
    }
}

async fn confirm_order(
    web::Path(hash): web::Path<String>,
    mollie: web::Data<mollie::Mollie>,
    mysql: web::Data<MySQL>,
) -> Result<HttpResponse> {
    let voucher_get = data::get_voucher(&mysql, data::Selector::ByHash(hash)).await;
    let voucher: Voucher;

    if voucher_get.is_none() {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/niet-gelukt")
            .finish());
    } else {
        voucher = voucher_get.unwrap();
    }

    if voucher.sale.paid {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/niet-gelukt")
            .finish());
    }

    let payment_id = voucher.sale.payment_id;
    let payment = mollie.get_payment(&payment_id).await;

    let transaction_fee = 1.5;
    let s = Bevestig {
        payment_url: payment.links.get("checkout").unwrap().href.to_string(),
        voucher_price: format!("{:.2}", voucher.sale.amount).replace(".", ","),
        total: format!("{:.2}", voucher.sale.amount + transaction_fee).replace(".", ","),
        from_str: format!(
            "{} {} <{}>",
            uppercase_first_letter(&voucher.sale.client.firstname),
            uppercase_first_letter(&voucher.sale.client.lastname),
            voucher.sale.client.email,
        ),
        to_str: format!(
            "{} <{}>",
            uppercase_first_letter(&voucher.receiver_name),
            voucher.receiver_email
        ),
    }
    .render()
    .unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn success(web::Path(action): web::Path<String>) -> Result<HttpResponse> {
    let (title, message) = match &*action {
        "aankoop" => (
            "Aankoop succesvol!",
            "We hebben je aankoop goed ontvangen en gaan direct voor je aan de slag!",
        ),
        "registratie" => (
            "Welkom bij Kaddo.",
            "Je bent succesvol geregistreerd, welkom!",
        ),
        _ => ("Success", "Goed gedaan!"),
    };

    let s = Success {
        title: title.to_string(),
        message: message.to_string(),
    }
    .render()
    .unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn failed() -> Result<HttpResponse> {
    let s = Failed.render().unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn voucher_desktop(
    web::Path(hash): web::Path<String>,
    mysql: web::Data<MySQL>,
) -> Result<HttpResponse> {
    let voucher = data::get_voucher(&mysql, data::Selector::ByHash(hash)).await;

    if voucher.is_none() {
        return Ok(HttpResponse::NotFound().finish());
    }

    let v = voucher.unwrap();

    let s = VoucherPageDesktop {
        distributor_name: v.distributorvoucher.distributor.name,
        balance: v.balance.to_string(),
        number_code: v.number_code,
        expiration_date: v.expiration_date.format("%d-%m-%Y").to_string(),
        receiver_name: v.receiver_name,
    }
    .render()
    .unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn voucher_mobile(
    web::Path(hash): web::Path<String>,
    mysql: web::Data<MySQL>,
) -> Result<HttpResponse> {
    let voucher = data::get_voucher(&mysql, data::Selector::ByHash(hash)).await;

    if voucher.is_none() {
        return Ok(HttpResponse::NotFound().finish());
    }

    let v = voucher.unwrap();

    let s = VoucherPageMobile {
        distributor_name: v.distributorvoucher.distributor.name,
        balance: v.balance.to_string(),
        number_code: v.number_code,
        expiration_date: v.expiration_date.format("%d-%m-%Y").to_string(),
        receiver_name: v.receiver_name,
    }
    .render()
    .unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn scanner(mysql: web::Data<MySQL>) -> Result<HttpResponse> {
    let s = Scanner.render().unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn scanner_login(mysql: web::Data<MySQL>) -> Result<HttpResponse> {
    let s = ScannerLogin.render().unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}
async fn scanner_login_authenticate(mysql: web::Data<MySQL>) -> Result<HttpResponse> {
    unimplemented!()
}

async fn scan_get(
    web::Path((method, identifier)): web::Path<(String, String)>,
    mysql: web::Data<MySQL>,
) -> String {
    let mut voucher: Option<Voucher> = None;

    // = data::get_voucher(&mysql, data::Selector::ByHash(hash)).await;

    match &*method {
        "hash" => {
            voucher = data::get_voucher(&mysql, data::Selector::ByHash(identifier)).await;
        }
        "number_code" => {
            voucher = data::get_voucher(&mysql, data::Selector::ByNumberCode(identifier)).await;
        }
        _ => {
            voucher = None;
        }
    }

    match voucher {
        None => "{\"success\": false, \"msg\": \"Geen voucher gevonden\", \"voucher\": null}"
            .to_string(),
        Some(v) => {
            format!(
                "{{
                \"success\": true,
                \"msg\": \"\",
                \"voucher\": {{
                    \"id\": {},
                    \"paid\": {},
                    \"one_use_only\": {},
                    \"used\": {},
                    \"balance\": {},
                    \"receiver_name\": \"{}\",
                    \"hash_code\": \"{}\"
                }}
            }}",
                v.id,
                v.sale.paid,
                v.distributorvoucher.one_use_only,
                v.used,
                v.balance,
                v.receiver_name,
                v.hash_code
            )
        }
    }
}

async fn scan_update(mysql: web::Data<MySQL>, form: web::Form<VoucherUpdateForm>) -> String {
    let mut voucher =
        data::get_voucher(&mysql, data::Selector::ByHash(form.hash.to_string())).await;

    match voucher {
        None => "error".to_string(),
        Some(mut v) => {
            v.used = form.used;
            v.balance = form.balance;

            match data::update_voucher(&mysql, &v).await {
                true => "success".to_string(),
                false => "false".to_string(),
            }
        }
    }
}

async fn test(mail: web::Data<mail::Mail>) -> Result<HttpResponse> {
    //mail.send_plain_mail().await;

    Ok(HttpResponse::Ok().finish())
}

// Administrator routes

async fn admin_login(
    mysql: web::Data<MySQL>,
    req: HttpRequest,
    session: Session,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_some()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/dashboard")
            .finish());
    }

    let distributor = data::get_distributor_by_subdomain(
        &mysql,
        get_subdomain_part(req.headers().get("Host").unwrap().to_str().unwrap()),
    )
    .await
    .unwrap();

    let s = AdminLogin {
        distributor_name: distributor.name,
        login_status: "".to_string(),
    }
    .render()
    .unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn admin_login_form(
    mysql: web::Data<MySQL>,
    req: HttpRequest,
    form: web::Form<AdminLoginForm>,
    session: Session,
) -> Result<HttpResponse> {
    let distributor = data::get_distributor_by_subdomain(
        &mysql,
        get_subdomain_part(req.headers().get("Host").unwrap().to_str().unwrap()),
    )
    .await
    .unwrap();

    let distributor_user = DistributorUser::get_by_username(&form.admin_username, &mysql).await;

    let mut s = AdminLogin {
        distributor_name: distributor.name,
        login_status: "".to_string(),
    };

    if distributor_user.is_none() {
        s.login_status = format!("Gebruiker {} niet gevonden.", &form.admin_username);
        return Ok(HttpResponse::NotFound()
            .content_type("text/html")
            .body(s.render().unwrap()));
    }
    if distributor_user.clone().unwrap().distributor.id != distributor.id {
        s.login_status = format!("Gebruiker {} niet gevonden.", &form.admin_username);
        return Ok(HttpResponse::NotFound()
            .content_type("text/html")
            .body(s.render().unwrap()));
    }

    let pass_check = distributor_user
        .clone()
        .unwrap()
        .verify_password(form.admin_password.as_bytes())
        .unwrap();

    match pass_check {
        true => {
            session.set("distributoruser", distributor_user.unwrap());

            Ok(HttpResponse::Found()
                .header(http::header::LOCATION, "/admin/dashboard")
                .finish())
        }
        false => {
            s.login_status = format!("Passwoord niet correct.");
            Ok(HttpResponse::Unauthorized()
                .content_type("text/html")
                .body(s.render().unwrap()))
        }
    }
}

async fn admin_dashboard_index(session: Session) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    return Ok(HttpResponse::Found()
        .header(http::header::LOCATION, "/admin/dashboard/mijn-zaak")
        .finish());
}

async fn admin_dashboard_mijn_zaak(
    session: Session,
    mysql: web::Data<MySQL>,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let mut s = AdminDashboardMijnZaak {
        distributor: get_distributor(
            &mysql,
            session
                .get::<DistributorUser>("distributoruser")?
                .unwrap()
                .distributor
                .id,
        )
        .await
        .unwrap(),
    }
    .render()
    .unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn admin_update_mijn_zaak(
    session: Session,
    mysql: web::Data<MySQL>,
    form: web::Form<MijnZaakUpdateForm>,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let mut distributor = session
        .get::<DistributorUser>("distributoruser")?
        .unwrap()
        .distributor;
    distributor.description = form.description.to_string();
    distributor.email = form.email.to_string();
    distributor.tel = form.tel.to_string();
    distributor.address = form.address.to_string();
    distributor.location = Location {
        id: 0,
        postalcode: form.postalcode.to_string(),
        city: form.city.to_string(),
    };

    match data::update_distributor(&mysql, &distributor).await {
        false => Ok(HttpResponse::BadRequest()
            .content_type("text/plain")
            .body("update_failed")),
        true => Ok(HttpResponse::Ok()
            .content_type("text/plain")
            .body("update_succeeded")),
    }
}

async fn admin_dashboard_cadeaubonnen(
    session: Session,
    mysql: web::Data<MySQL>,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let all_distributor_vouchers: Vec<DistributorVoucher> =
        data::get_distributor_vouchers_by_distributor(
            &mysql,
            session
                .get::<DistributorUser>("distributoruser")
                .unwrap()
                .unwrap()
                .distributor
                .id,
        )
        .await
        .unwrap();
    let mut three_price_distributor_vouchers: Vec<&DistributorVoucher> = Vec::new();
    let mut price_range_distributor_voucher: Option<&DistributorVoucher> = None;
    let mut label_distributor_vouchers: Vec<LabelVoucherData> = Vec::new();
    let mut currently_active: Option<VoucherType> = None;
    for distributor_voucher in all_distributor_vouchers.iter() {
        if distributor_voucher.voucher_type == VoucherType::ThreeOptionVoucher {
            three_price_distributor_vouchers.push(distributor_voucher);

            if distributor_voucher.active {
                currently_active = Some(VoucherType::ThreeOptionVoucher);
            }
        }

        if distributor_voucher.voucher_type == VoucherType::RangeVoucher {
            price_range_distributor_voucher = Some(&distributor_voucher);

            if distributor_voucher.active {
                currently_active = Some(VoucherType::RangeVoucher);
            }
        }

        if distributor_voucher.voucher_type == VoucherType::LabelVoucher {
            label_distributor_vouchers.push(LabelVoucherData {
                title: distributor_voucher.label.to_string(),
                amount: distributor_voucher.amount as u64,
                description: distributor_voucher.description.to_string(),
                days_valid: distributor_voucher.days_valid,
            });

            if distributor_voucher.active {
                currently_active = Some(VoucherType::LabelVoucher);
            }
        }
    }

    let mut s = AdminDashboardCadeaubonnen {
        three_price_voucher_a: if three_price_distributor_vouchers.len() > 0 {
            three_price_distributor_vouchers[0].amount
        } else {
            10 as f64
        },
        three_price_voucher_b: if three_price_distributor_vouchers.len() > 0 {
            three_price_distributor_vouchers[1].amount
        } else {
            25 as f64
        },
        three_price_voucher_c: if three_price_distributor_vouchers.len() > 0 {
            three_price_distributor_vouchers[2].amount
        } else {
            50 as f64
        },
        three_price_voucher_max_days: if three_price_distributor_vouchers.len() > 0 {
            three_price_distributor_vouchers[0].days_valid
        } else {
            90
        },
        three_price_voucher_one_use_only: if three_price_distributor_vouchers.len() > 0 {
            three_price_distributor_vouchers[0].one_use_only
        } else {
            false
        },

        price_range_voucher_min: if price_range_distributor_voucher.is_some() {
            price_range_distributor_voucher.unwrap().min_amount
        } else {
            10 as f64
        },
        price_range_voucher_max: if price_range_distributor_voucher.is_some() {
            price_range_distributor_voucher.unwrap().max_amount
        } else {
            50 as f64
        },
        price_range_voucher_auto: if price_range_distributor_voucher.is_some() {
            price_range_distributor_voucher.unwrap().amount
        } else {
            20 as f64
        },
        price_range_voucher_max_days: if price_range_distributor_voucher.is_some() {
            price_range_distributor_voucher.unwrap().days_valid
        } else {
            90
        },
        price_range_voucher_one_use_only: if price_range_distributor_voucher.is_some() {
            price_range_distributor_voucher.unwrap().one_use_only
        } else {
            false
        },

        label_vouchers: &label_distributor_vouchers,
        label_voucher_max_days: if label_distributor_vouchers.len() > 0 {
            label_distributor_vouchers[0].days_valid
        } else {
            90
        },

        currently_active: currently_active,
    }
    .render()
    .unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn admin_update_cadeaubonnen(
    session: Session,
    req: HttpRequest,
    mysql: web::Data<MySQL>,
    json: web::Json<AdminVouchersUpdateJson>,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let voucher_type: VoucherType =
        VoucherType::from_str(req.match_info().get("voucher_type").unwrap()).unwrap();

    let mut vouchers_to_add: std::vec::Vec<DistributorVoucher> = Vec::new();
    match voucher_type {
        VoucherType::ThreeOptionVoucher => {
            for distributor_voucher in &json.three_option_vouchers {
                vouchers_to_add.push(DistributorVoucher {
                    id: 0,
                    distributor: session
                        .get::<DistributorUser>("distributoruser")?
                        .unwrap()
                        .distributor,
                    voucher_type: VoucherType::ThreeOptionVoucher,
                    amount: *distributor_voucher as f64,
                    min_amount: 0 as f64,
                    max_amount: 0 as f64,
                    label: "".to_string(),
                    description: "".to_string(),
                    days_valid: json.days_valid as u16,
                    active: true,
                    one_use_only: json.one_use_only as bool,
                    create_date: None,
                });
            }
        }
        VoucherType::RangeVoucher => {
            vouchers_to_add.push(DistributorVoucher {
                id: 0,
                distributor: session
                    .get::<DistributorUser>("distributoruser")?
                    .unwrap()
                    .distributor,
                voucher_type: VoucherType::RangeVoucher,
                amount: json.price_range_voucher.auto_amount as f64,
                min_amount: json.price_range_voucher.min_amount as f64,
                max_amount: json.price_range_voucher.max_amount as f64,
                label: "".to_string(),
                description: "".to_string(),
                days_valid: json.days_valid as u16,
                active: true,
                one_use_only: json.one_use_only as bool,
                create_date: None,
            });
        }
        VoucherType::LabelVoucher => {
            for distributor_voucher in &json.label_vouchers {
                vouchers_to_add.push(DistributorVoucher {
                    id: 0,
                    distributor: session
                        .get::<DistributorUser>("distributoruser")?
                        .unwrap()
                        .distributor,
                    voucher_type: VoucherType::LabelVoucher,
                    amount: distributor_voucher.amount as f64,
                    min_amount: 0 as f64,
                    max_amount: 0 as f64,
                    label: distributor_voucher.title.to_string(),
                    description: distributor_voucher.description.to_string(),
                    days_valid: json.days_valid as u16,
                    active: true,
                    one_use_only: true,
                    create_date: None,
                });
            }
        }
        _ => {
            return Ok(HttpResponse::BadRequest().finish());
        }
    }

    data::add_active_distributor_vouchers(&mysql, vouchers_to_add).await;

    Ok(HttpResponse::Ok().content_type("text/plain").body(""))
}

async fn admin_dashboard_bestellingen(
    session: Session,
    mysql: web::Data<MySQL>,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let s = AdminDashboardBestellingen.render().unwrap();
    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn admin_dashboard_bestelling(
    session: Session,
    mysql: web::Data<MySQL>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let id = req.match_info().get("id").unwrap().parse::<u64>().unwrap();
    let voucher: Voucher = data::get_voucher(&mysql, data::Selector::ById(id))
        .await
        .unwrap();

    let s = AdminOrderData { voucher: voucher }.render().unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn admin_dashboard_get_bestellingen(
    session: Session,
    mysql: web::Data<MySQL>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let query_str = req.query_string().replace("%20", " ");
    let filters = web::Query::<AdminOrderFilterParams>::from_query(&query_str)
        .unwrap()
        .into_inner();

    let orders = data::get_all_orders(
        &mysql,
        filters.amount,
        25,
        filters,
        session
            .get::<DistributorUser>("distributoruser")
            .unwrap()
            .unwrap()
            .distributor
            .id,
    )
    .await;

    Ok(HttpResponse::Ok().json(&orders))
}

async fn admin_dashboard_wachtwoord(session: Session) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let mut s = AdminDashboardWachtwoord.render().unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn admin_dashboard_help(session: Session) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_none()
    {
        return Ok(HttpResponse::Found()
            .header(http::header::LOCATION, "/admin/login")
            .finish());
    }

    let mut s = AdminDashboardHelp.render().unwrap();

    Ok(HttpResponse::Ok().content_type("text/html").body(s))
}

async fn admin_dashboard_logout(session: Session) -> Result<HttpResponse> {
    if session
        .get::<DistributorUser>("distributoruser")
        .unwrap()
        .is_some()
    {
        session.remove("distributoruser");
    }

    Ok(HttpResponse::Found()
        .header(http::header::LOCATION, "/admin/login")
        .finish())
}

/* DEFAULT ROUTES AND ERRORS */
async fn error404() -> Result<HttpResponse> {
    let s = Error404.render().unwrap();
    Ok(HttpResponse::NotFound().content_type("text/html").body(s))
}

/* SEED FUNCTIONS */
async fn seed(mysql: MySQL) {
    println!("[+] Started seeding...");
    seed_distributor_user(mysql).await;
}

async fn seed_distributor_user(mysql: MySQL) {
    let sql = "DELETE FROM distributoruser";
    let mut result = sqlx::query(&sql).execute(&mysql.conn).await;

    let mut user1 = DistributorUser {
        id: 0,
        username: "demok_admin".to_string(),
        password: "test123".to_string(),
        distributor: get_distributor(&web::Data::new(mysql.clone()), 1)
            .await
            .unwrap(),
        display_name: "Jouw Naam".to_string(),
    };

    user1.id = DistributorUser::create(&mut user1, &web::Data::new(mysql)).await;

    println!(
        "[+] Added user {} (#{}) to distributoruser",
        user1.username, user1.id
    );
}

// test
async fn clear(session: Session) -> Result<HttpResponse> {
    session.clear();

    Ok(HttpResponse::NoContent().await?)
}

/* MAIN */
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let conn_str: String = format!("mysql://test@localhost/kaddo");

    let mysql = MySQL {
        conn: MySqlPool::connect(&conn_str).await.unwrap(),
    };
    let mollie = mollie::Mollie {
        api_key: "test_8v3dnuvdFp7hj6U9TCvtCpr8MsNeHx".to_string(),
    };
    let mail = mail::Mail {
        creds: mail::Creds {
            username: "".to_string(),
            password: "".to_string(),
        },
        smtp_server: "".to_string(),
    };

    // Seed
    if find_arg(&"seed".to_string()).await {
        seed(mysql.clone()).await;
    }

    // start http server
    HttpServer::new(move || {
        App::new()
            .data(mysql.clone())
            .data(mollie.clone())
            .data(mail.clone())
            .wrap(CookieSession::signed(&[0; 32]).secure(false))
            // Business services
            .service(web::resource("/").route(web::get().to(index)))
            .service(web::resource("/home").route(web::get().to(index)))
            .service(web::resource("/bestel").route(web::get().to(bestel)))
            .service(web::resource("/order_form").route(web::post().to(order_form)))
            .service(web::resource("/bevestig/{hash}").route(web::get().to(confirm_order)))
            .service(web::resource("/faq").route(web::get().to(faq)))
            .service(web::resource("/payment/hook").route(web::post().to(payment_hook))) // Mollie hook
            .service(web::resource("/check/{payment_id}").route(web::get().to(check)))
            .service(web::resource("/succes/{action}").route(web::get().to(success)))
            .service(web::resource("/niet-gelukt").route(web::get().to(failed)))
            .service(web::resource("/bon/{hash}").route(web::get().to(voucher_desktop))) // Bon
            .service(web::resource("/mobile/bon/{hash}").route(web::get().to(voucher_mobile))) // Bon (Mobile)
            .service(web::resource("/scanner").route(web::get().to(scanner)))
            .service(
                web::resource("/scanner/login")
                    .route(web::get().to(scanner_login))
                    .route(web::post().to(scanner_login_authenticate)),
            )
            .service(
                web::resource("/scan/get/{method}/{identifier}").route(web::get().to(scan_get)),
            )
            .service(web::resource("/scan/update").route(web::post().to(scan_update)))
            .service(web::resource("/test").route(web::get().to(test))) // TEST
            // Administrator services
            .service(
                web::resource("/admin/login")
                    .route(web::get().to(admin_login))
                    .route(web::post().to(admin_login_form)),
            )
            .service(web::resource("/admin").route(web::get().to(admin_dashboard_index)))
            .service(web::resource("/admin/").route(web::get().to(admin_dashboard_index)))
            .service(web::resource("/admin/dashboard").route(web::get().to(admin_dashboard_index)))
            .service(web::resource("/admin/dashboard/").route(web::get().to(admin_dashboard_index)))
            .service(
                web::resource("/admin/dashboard/mijn-zaak")
                    .route(web::get().to(admin_dashboard_mijn_zaak)),
            )
            .service(
                web::resource("/admin/dashboard/mijn-zaak/update")
                    .route(web::post().to(admin_update_mijn_zaak)),
            )
            .service(
                web::resource("/admin/dashboard/cadeaubonnen")
                    .route(web::get().to(admin_dashboard_cadeaubonnen)),
            )
            .service(
                web::resource("/admin/dashboard/cadeaubonnen/update/{voucher_type}")
                    .route(web::post().to(admin_update_cadeaubonnen)),
            )
            .service(
                web::resource("/admin/dashboard/bestellingen")
                    .route(web::get().to(admin_dashboard_bestellingen)),
            )
            .service(
                web::resource("/admin/dashboard/bestellingen/get")
                    .route(web::get().to(admin_dashboard_get_bestellingen)),
            )
            .service(
                web::resource("/admin/dashboard/bestellingen/{id}")
                    .route(web::get().to(admin_dashboard_bestelling)),
            )
            .service(
                web::resource("/admin/dashboard/wachtwoord")
                    .route(web::get().to(admin_dashboard_wachtwoord)),
            )
            .service(
                web::resource("/admin/dashboard/help").route(web::get().to(admin_dashboard_help)),
            )
            .service(
                web::resource("/admin/dashboard/uitloggen")
                    .route(web::get().to(admin_dashboard_logout)),
            )
            // General
            .service(Files::new("/assets", "./templates/assets").show_files_listing())
            .service(web::resource("/clear").route(web::get().to(clear)))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

// Function to check if an argument is set
async fn find_arg(arg_to_find: &String) -> bool {
    let args: Vec<String> = env::args().collect();
    args.contains(arg_to_find)
}

// Function to capitalize string
fn uppercase_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}
