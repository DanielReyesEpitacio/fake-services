//! Datos ficticios simples, sin dependencias y orientados a México/LatAm.

pub const KINDS: &[&str] = &[
    "first_name",
    "last_name",
    "full_name",
    "email",
    "phone",
    "company",
    "city",
    "street",
    "zip",
    "rfc",
    "curp",
    "clabe",
    "word",
    "sentence",
    "iban",
];

const FIRST: &[&str] = &[
    "María",
    "José",
    "Juan",
    "Guadalupe",
    "Ana",
    "Luis",
    "Sofía",
    "Carlos",
    "Valeria",
    "Miguel",
    "Fernanda",
    "Diego",
    "Camila",
    "Jorge",
    "Daniela",
    "Andrés",
    "Lucía",
    "Ricardo",
    "Paola",
    "Emilio",
];
const LAST: &[&str] = &[
    "Hernández",
    "García",
    "Martínez",
    "López",
    "González",
    "Pérez",
    "Rodríguez",
    "Sánchez",
    "Ramírez",
    "Cruz",
    "Flores",
    "Gómez",
    "Morales",
    "Vázquez",
    "Reyes",
    "Jiménez",
    "Torres",
    "Díaz",
    "Ruiz",
    "Mendoza",
];
const COMPANY: &[&str] = &[
    "Comercializadora del Bajío",
    "Servicios Integrales Aztlán",
    "Grupo Pacífico",
    "Tecnologías Quetzal",
    "Logística del Norte",
    "Consultores Maya",
    "Alimentos La Huerta",
    "Finanzas Sol",
    "Inmobiliaria Roble",
    "Textiles Oaxaca",
];
const SUFFIX: &[&str] = &[
    "S.A. de C.V.",
    "S.A.P.I. de C.V.",
    "S. de R.L. de C.V.",
    "S.C.",
];
const CITY: &[&str] = &[
    "Ciudad de México",
    "Guadalajara",
    "Monterrey",
    "Puebla",
    "Querétaro",
    "Mérida",
    "León",
    "Tijuana",
    "Oaxaca",
    "Morelia",
];
const STREET: &[&str] = &[
    "Av. Reforma",
    "Insurgentes Sur",
    "Calle Hidalgo",
    "Av. Juárez",
    "Calle Morelos",
    "Av. Universidad",
    "Calle 5 de Mayo",
    "Av. Revolución",
];
const WORDS: &[&str] = &[
    "documento",
    "firma",
    "cuenta",
    "pago",
    "factura",
    "cliente",
    "servicio",
    "contrato",
    "transacción",
    "saldo",
    "reporte",
    "solicitud",
    "registro",
    "proceso",
    "estado",
];

fn pick(list: &[&str]) -> String {
    list[fastrand::usize(0..list.len())].to_string()
}

fn ascii_fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'Á' => 'a',
            'é' | 'É' => 'e',
            'í' | 'Í' => 'i',
            'ó' | 'Ó' => 'o',
            'ú' | 'Ú' | 'ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            c => c,
        })
        .collect()
}

fn digits(n: usize) -> String {
    std::iter::repeat_with(|| char::from(b'0' + fastrand::u8(0..10)))
        .take(n)
        .collect()
}

fn letters(n: usize) -> String {
    std::iter::repeat_with(|| char::from(b'A' + fastrand::u8(0..26)))
        .take(n)
        .collect()
}

fn date6() -> String {
    format!(
        "{:02}{:02}{:02}",
        fastrand::u8(60..100),
        fastrand::u8(1..13),
        fastrand::u8(1..29)
    )
}

pub fn generate(kind: &str) -> Option<String> {
    Some(match kind {
        "first_name" => pick(FIRST),
        "last_name" => pick(LAST),
        "full_name" => format!("{} {} {}", pick(FIRST), pick(LAST), pick(LAST)),
        "email" => format!(
            "{}.{}{}@example.com",
            ascii_fold(&pick(FIRST)).to_lowercase(),
            ascii_fold(&pick(LAST)).to_lowercase(),
            fastrand::u16(1..100)
        ),
        "phone" => format!("+52 55 {} {}", digits(4), digits(4)),
        "company" => format!("{} {}", pick(COMPANY), pick(SUFFIX)),
        "city" => pick(CITY),
        "street" => format!("{} {}", pick(STREET), fastrand::u16(1..2000)),
        "zip" => digits(5),
        "rfc" => format!("{}{}{}", letters(4), date6(), letters(1) + &digits(2)),
        "curp" => format!(
            "{}{}{}{}{}",
            letters(4),
            date6(),
            if fastrand::bool() { "H" } else { "M" },
            letters(5),
            digits(2)
        ),
        "clabe" => digits(18),
        "iban" => format!("MX{}", digits(20)),
        "word" => pick(WORDS),
        "sentence" => {
            let n = fastrand::usize(4..9);
            let mut s: Vec<String> = (0..n).map(|_| pick(WORDS)).collect();
            if let Some(first) = s.first_mut() {
                let mut c = first.chars();
                if let Some(f) = c.next() {
                    *first = f.to_uppercase().collect::<String>() + c.as_str();
                }
            }
            format!("{}.", s.join(" "))
        }
        _ => return None,
    })
}
