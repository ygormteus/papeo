//! PIN ou senha do próprio Papeo, para bloquear o app em computadores sem
//! Touch ID/senha do sistema ou Windows Hello. Só o hash Argon2id (com sal)
//! é guardado, em `config.json`; nunca entra no backup nem na sincronização.
//!
//! `verify` leva dezenas de milissegundos de propósito: chame fora da thread
//! principal e sem segurar o lock do estado.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};

pub const PIN_MIN: usize = 4;
pub const PIN_MAX: usize = 64;
/// Erros seguidos antes de começar a esperar entre tentativas.
pub const PIN_FREE_TRIES: u32 = 5;
/// Maior espera entre tentativas, em segundos.
const PIN_MAX_WAIT: u64 = 15 * 60;

// Parâmetros recomendados pela OWASP para Argon2id (19 MiB, 2 passadas).
const MEMORY_KIB: u32 = 19 * 1024;
const PASSES: u32 = 2;
const LANES: u32 = 1;
const PREFIX: &str = "argon2id";

/// PIN guardado: hash e erros seguidos (persistidos para que fechar e abrir
/// o app não zere a espera entre tentativas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockPin {
    /// `argon2id$<memória KiB>$<passadas>$<faixas>$<sal>$<hash>` (base64).
    pub hash: String,
    #[serde(default)]
    pub failures: u32,
}

/// Regras do PIN/senha: de 4 a 64 caracteres, sem caracteres de controle e
/// sem ser só espaços.
pub fn check_pin(pin: &str) -> Result<(), String> {
    let count = pin.chars().count();
    if count < PIN_MIN {
        return Err(format!("Use pelo menos {PIN_MIN} caracteres."));
    }
    if count > PIN_MAX {
        return Err(format!("Use no máximo {PIN_MAX} caracteres."));
    }
    if pin.chars().any(char::is_control) || pin.trim().is_empty() {
        return Err("O PIN tem caracteres inválidos.".into());
    }
    Ok(())
}

fn derive(pin: &str, salt: &[u8], memory: u32, passes: u32, lanes: u32) -> Option<[u8; 32]> {
    // Limites para não travar o app com um arquivo adulterado.
    if !(8 * 1024..=256 * 1024).contains(&memory)
        || !(1..=4).contains(&passes)
        || !(1..=4).contains(&lanes)
        || !(8..=64).contains(&salt.len())
    {
        return None;
    }
    let params = Params::new(memory, passes, lanes, Some(32)).ok()?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), salt, &mut out)
        .ok()?;
    Some(out)
}

struct Parsed {
    memory: u32,
    passes: u32,
    lanes: u32,
    salt: Vec<u8>,
    hash: Vec<u8>,
}

fn parse(encoded: &str) -> Option<Parsed> {
    let mut parts = encoded.split('$');
    if parts.next()? != PREFIX {
        return None;
    }
    let memory = parts.next()?.parse().ok()?;
    let passes = parts.next()?.parse().ok()?;
    let lanes = parts.next()?.parse().ok()?;
    let salt = B64.decode(parts.next()?).ok()?;
    let hash = B64.decode(parts.next()?).ok()?;
    if parts.next().is_some() || hash.len() != 32 {
        return None;
    }
    Some(Parsed {
        memory,
        passes,
        lanes,
        salt,
        hash,
    })
}

/// Compara sem sair cedo (o tempo não revela quantos bytes acertaram).
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl LockPin {
    /// Gera o hash de um PIN novo (já validado com `check_pin`).
    pub fn new(pin: &str) -> Result<Self, String> {
        check_pin(pin)?;
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).map_err(|e| e.to_string())?;
        let hash = derive(pin, &salt, MEMORY_KIB, PASSES, LANES)
            .ok_or("Não foi possível proteger o PIN.")?;
        Ok(Self {
            hash: format!(
                "{PREFIX}${MEMORY_KIB}${PASSES}${LANES}${}${}",
                B64.encode(salt),
                B64.encode(hash)
            ),
            failures: 0,
        })
    }

    /// O hash guardado está num formato que o app entende.
    pub fn is_valid(&self) -> bool {
        parse(&self.hash).is_some_and(|p| {
            (8 * 1024..=256 * 1024).contains(&p.memory)
                && (1..=4).contains(&p.passes)
                && (1..=4).contains(&p.lanes)
                && (8..=64).contains(&p.salt.len())
        })
    }

    /// Confere o PIN digitado. Lento de propósito (Argon2id).
    pub fn verify(&self, pin: &str) -> bool {
        let Some(p) = parse(&self.hash) else {
            return false;
        };
        derive(pin, &p.salt, p.memory, p.passes, p.lanes).is_some_and(|h| same(&h, &p.hash))
    }

    pub fn sanitized(mut self) -> Option<Self> {
        self.failures = self.failures.min(1000);
        self.is_valid().then_some(self)
    }
}

/// Espera, em segundos, antes da próxima tentativa depois de `failures`
/// erros seguidos: nenhuma nas primeiras, depois 30 s, 1 min, 2 min… até 15.
pub fn pin_wait_secs(failures: u32) -> u64 {
    if failures < PIN_FREE_TRIES {
        return 0;
    }
    let step = (failures - PIN_FREE_TRIES).min(10);
    (30u64 << step).min(PIN_MAX_WAIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_only_the_right_pin() {
        let pin = LockPin::new("1234").unwrap();
        assert!(pin.is_valid());
        assert!(pin.verify("1234"));
        assert!(!pin.verify("1235"));
        assert!(!pin.verify(""));
        assert_ne!(
            LockPin::new("1234").unwrap().hash,
            pin.hash,
            "sal aleatório"
        );
    }

    #[test]
    fn rejects_weak_or_odd_pins() {
        assert!(check_pin("123").is_err());
        assert!(check_pin("    ").is_err());
        assert!(check_pin("12\n34").is_err());
        assert!(check_pin(&"a".repeat(PIN_MAX + 1)).is_err());
        assert!(check_pin("senha boa").is_ok());
        assert!(check_pin("ação").is_ok());
    }

    #[test]
    fn rejects_tampered_hashes() {
        let good = LockPin::new("123456").unwrap();
        for bad in [
            "",
            "argon2id$1$2$1$AAAAAAAAAAAAAAAAAAAAAA$AAAA",
            "bcrypt$19456$2$1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ] {
            let pin = LockPin {
                hash: bad.into(),
                failures: 0,
            };
            assert!(!pin.is_valid());
            assert!(!pin.verify("123456"));
            assert!(pin.sanitized().is_none());
        }
        // Memória absurda (travaria o app).
        let huge = good.hash.replacen("$19456$", "$4000000$", 1);
        let pin = LockPin {
            hash: huge,
            failures: 0,
        };
        assert!(!pin.is_valid());
        assert!(!pin.verify("123456"));
    }

    #[test]
    fn waits_grow_after_free_tries() {
        assert_eq!(pin_wait_secs(0), 0);
        assert_eq!(pin_wait_secs(4), 0);
        assert_eq!(pin_wait_secs(5), 30);
        assert_eq!(pin_wait_secs(6), 60);
        assert_eq!(pin_wait_secs(7), 120);
        assert_eq!(pin_wait_secs(20), 15 * 60);
        assert_eq!(pin_wait_secs(u32::MAX), 15 * 60);
    }
}
