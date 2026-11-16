//! O protocolo entre o mod e o servidor de amigos (`servidor/`).
//!
//! Uma mensagem é uma linha de texto UTF-8 terminada em `\n`. Os campos são
//! separados por tabulação; dentro de um campo, `\\`, `\t` e `\n` são
//! escritos como `\\\\`, `\\t` e `\\n`. O primeiro campo diz o que a
//! mensagem é. Este arquivo é o mesmo nos dois lados: o servidor o inclui
//! com `#[path]`, então os dois nunca discordam do formato.

// Cada lado usa metade: o mod codifica `DoCliente` e lê `DoServidor`, o servidor o contrário.
#![allow(dead_code)]

/// Versão do protocolo. O servidor recusa clientes de outra versão.
pub const VERSAO: u32 = 1;

/// Tamanho máximo de uma linha, em bytes, sem o `\n`.
pub const LINHA_MAX: usize = 4096;

/// Tamanho máximo de uma mensagem de chat entre amigos, em caracteres.
pub const TEXTO_MAX: usize = 256;

/// Tamanho máximo do status ("Jogando em ..."), em caracteres.
pub const STATUS_MAX: usize = 64;

/// Porta padrão do servidor de amigos.
pub const PORTA_PADRAO: u16 = 25590;

/// O que o mod manda ao servidor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DoCliente {
    /// Entrar com um nome e o segredo deste computador. O primeiro a entrar
    /// com um nome fica com ele; depois só o mesmo segredo entra.
    Ola {
        nome: String,
        segredo: String,
        versao: u32,
    },
    /// Pedir amizade (ou aceitar, se a outra pessoa já pediu).
    Adicionar { nome: String },
    /// Aceitar um pedido recebido.
    Aceitar { nome: String },
    /// Recusar um pedido recebido.
    Recusar { nome: String },
    /// Desfazer uma amizade (ou cancelar um pedido enviado).
    Remover { nome: String },
    /// Mensagem privada para um amigo.
    Mensagem { para: String, texto: String },
    /// Convite para o mundo: o código do "Jogar com amigos" do Horizon.
    Convite { para: String, codigo: String },
    /// O que estou fazendo agora, mostrado aos amigos.
    Status { texto: String },
    /// Mantém a conexão viva.
    Ping,
}

/// O que o servidor manda ao mod.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DoServidor {
    /// Entrada aceita, com o nome como o servidor o guarda.
    BemVindo { nome: String },
    /// Algo deu errado; `texto` é para mostrar ao jogador.
    Erro { texto: String },
    /// Um amigo e o estado dele (mandado ao entrar e a cada mudança).
    Amigo {
        nome: String,
        online: bool,
        status: String,
    },
    /// Alguém deixou de ser amigo.
    AmigoRemovido { nome: String },
    /// Alguém pediu amizade.
    PedidoRecebido { nome: String },
    /// Um pedido que eu fiz e ainda espera resposta.
    PedidoEnviado { nome: String },
    /// Um pedido (recebido ou enviado) que deixou de existir.
    PedidoRemovido { nome: String },
    /// Mensagem de um amigo.
    Mensagem { de: String, texto: String },
    /// Convite de um amigo para o mundo dele.
    Convite { de: String, codigo: String },
    /// Resposta a `Ping`.
    Pong,
}

/// Por que uma linha não foi entendida.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErroLinha {
    /// Linha maior que [`LINHA_MAX`].
    Grande,
    /// Linha vazia ou com tipo desconhecido.
    Desconhecida,
    /// Tipo certo, mas campos faltando ou a mais, ou um número inválido.
    Campos,
    /// Escape inválido (`\` seguido de outra coisa).
    Escape,
}

impl std::fmt::Display for ErroLinha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Grande => "linha grande demais",
            Self::Desconhecida => "mensagem desconhecida",
            Self::Campos => "campos errados",
            Self::Escape => "escape inválido",
        })
    }
}

fn escapar(campo: &str, saida: &mut String) {
    for c in campo.chars() {
        match c {
            '\\' => saida.push_str("\\\\"),
            '\t' => saida.push_str("\\t"),
            '\n' => saida.push_str("\\n"),
            '\r' => saida.push_str("\\r"),
            c => saida.push(c),
        }
    }
}

fn desescapar(campo: &str) -> Result<String, ErroLinha> {
    let mut saida = String::with_capacity(campo.len());
    let mut chars = campo.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            saida.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => saida.push('\\'),
            Some('t') => saida.push('\t'),
            Some('n') => saida.push('\n'),
            Some('r') => saida.push('\r'),
            _ => return Err(ErroLinha::Escape),
        }
    }
    Ok(saida)
}

/// Junta os campos numa linha, com o `\n` no fim.
fn linha(campos: &[&str]) -> String {
    let mut saida = String::new();
    for (i, campo) in campos.iter().enumerate() {
        if i > 0 {
            saida.push('\t');
        }
        escapar(campo, &mut saida);
    }
    saida.push('\n');
    saida
}

/// Separa uma linha (sem o `\n`) nos seus campos.
fn campos(linha: &str) -> Result<Vec<String>, ErroLinha> {
    if linha.len() > LINHA_MAX {
        return Err(ErroLinha::Grande);
    }
    let linha = linha.strip_suffix('\r').unwrap_or(linha);
    if linha.is_empty() {
        return Err(ErroLinha::Desconhecida);
    }
    linha.split('\t').map(desescapar).collect()
}

fn bool_campo(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

impl DoCliente {
    /// A mensagem como uma linha, com o `\n` no fim.
    pub fn codificar(&self) -> String {
        match self {
            Self::Ola {
                nome,
                segredo,
                versao,
            } => linha(&["OLA", nome, segredo, &versao.to_string()]),
            Self::Adicionar { nome } => linha(&["ADICIONAR", nome]),
            Self::Aceitar { nome } => linha(&["ACEITAR", nome]),
            Self::Recusar { nome } => linha(&["RECUSAR", nome]),
            Self::Remover { nome } => linha(&["REMOVER", nome]),
            Self::Mensagem { para, texto } => linha(&["MSG", para, texto]),
            Self::Convite { para, codigo } => linha(&["CONVITE", para, codigo]),
            Self::Status { texto } => linha(&["STATUS", texto]),
            Self::Ping => linha(&["PING"]),
        }
    }

    /// Lê uma linha (sem o `\n`).
    pub fn decodificar(texto: &str) -> Result<Self, ErroLinha> {
        let c = campos(texto)?;
        let s: Vec<&str> = c.iter().map(String::as_str).collect();
        let um = |f: fn(String) -> Self| match s.as_slice() {
            [_, a] => Ok(f((*a).to_owned())),
            _ => Err(ErroLinha::Campos),
        };
        match s.as_slice() {
            ["OLA", nome, segredo, versao] => Ok(Self::Ola {
                nome: (*nome).to_owned(),
                segredo: (*segredo).to_owned(),
                versao: versao.parse().map_err(|_| ErroLinha::Campos)?,
            }),
            ["OLA", ..] => Err(ErroLinha::Campos),
            ["ADICIONAR", ..] => um(|nome| Self::Adicionar { nome }),
            ["ACEITAR", ..] => um(|nome| Self::Aceitar { nome }),
            ["RECUSAR", ..] => um(|nome| Self::Recusar { nome }),
            ["REMOVER", ..] => um(|nome| Self::Remover { nome }),
            ["STATUS", ..] => um(|texto| Self::Status { texto }),
            ["MSG", para, texto] => Ok(Self::Mensagem {
                para: (*para).to_owned(),
                texto: (*texto).to_owned(),
            }),
            ["CONVITE", para, codigo] => Ok(Self::Convite {
                para: (*para).to_owned(),
                codigo: (*codigo).to_owned(),
            }),
            ["PING"] => Ok(Self::Ping),
            ["MSG" | "CONVITE" | "PING", ..] => Err(ErroLinha::Campos),
            _ => Err(ErroLinha::Desconhecida),
        }
    }
}

impl DoServidor {
    /// A mensagem como uma linha, com o `\n` no fim.
    pub fn codificar(&self) -> String {
        match self {
            Self::BemVindo { nome } => linha(&["BEMVINDO", nome]),
            Self::Erro { texto } => linha(&["ERRO", texto]),
            Self::Amigo {
                nome,
                online,
                status,
            } => linha(&["AMIGO", nome, bool_campo(*online), status]),
            Self::AmigoRemovido { nome } => linha(&["AMIGO_REMOVIDO", nome]),
            Self::PedidoRecebido { nome } => linha(&["PEDIDO_RECEBIDO", nome]),
            Self::PedidoEnviado { nome } => linha(&["PEDIDO_ENVIADO", nome]),
            Self::PedidoRemovido { nome } => linha(&["PEDIDO_REMOVIDO", nome]),
            Self::Mensagem { de, texto } => linha(&["MSG", de, texto]),
            Self::Convite { de, codigo } => linha(&["CONVITE", de, codigo]),
            Self::Pong => linha(&["PONG"]),
        }
    }

    /// Lê uma linha (sem o `\n`).
    pub fn decodificar(texto: &str) -> Result<Self, ErroLinha> {
        let c = campos(texto)?;
        let s: Vec<&str> = c.iter().map(String::as_str).collect();
        let um = |f: fn(String) -> Self| match s.as_slice() {
            [_, a] => Ok(f((*a).to_owned())),
            _ => Err(ErroLinha::Campos),
        };
        match s.as_slice() {
            ["BEMVINDO", ..] => um(|nome| Self::BemVindo { nome }),
            ["ERRO", ..] => um(|texto| Self::Erro { texto }),
            ["AMIGO", nome, online, status] => Ok(Self::Amigo {
                nome: (*nome).to_owned(),
                online: match *online {
                    "1" => true,
                    "0" => false,
                    _ => return Err(ErroLinha::Campos),
                },
                status: (*status).to_owned(),
            }),
            ["AMIGO_REMOVIDO", ..] => um(|nome| Self::AmigoRemovido { nome }),
            ["PEDIDO_RECEBIDO", ..] => um(|nome| Self::PedidoRecebido { nome }),
            ["PEDIDO_ENVIADO", ..] => um(|nome| Self::PedidoEnviado { nome }),
            ["PEDIDO_REMOVIDO", ..] => um(|nome| Self::PedidoRemovido { nome }),
            ["MSG", de, texto] => Ok(Self::Mensagem {
                de: (*de).to_owned(),
                texto: (*texto).to_owned(),
            }),
            ["CONVITE", de, codigo] => Ok(Self::Convite {
                de: (*de).to_owned(),
                codigo: (*codigo).to_owned(),
            }),
            ["PONG"] => Ok(Self::Pong),
            ["AMIGO" | "MSG" | "CONVITE" | "PONG", ..] => Err(ErroLinha::Campos),
            _ => Err(ErroLinha::Desconhecida),
        }
    }
}

/// Um nome de jogador válido: 3 a 16 letras, números ou `_`, como os do
/// Minecraft.
pub fn nome_valido(nome: &str) -> bool {
    (3..=16).contains(&nome.len()) && nome.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Um código de convite do "Jogar com amigos" do Horizon, como o botão
/// "Copiar código" o copia: oito símbolos em base32 Crockford (`K7QM-3XPD`)
/// ou a forma longa com os endereços (só letras, números e `-`).
/// Devolve o código sem espaços em volta, ou `None` se não parece um.
pub fn codigo_convite(texto: &str) -> Option<String> {
    let codigo = texto.trim();
    let codigo = codigo.strip_prefix("friend:").unwrap_or(codigo);
    let simbolos = codigo.bytes().filter(|b| *b != b'-').count();
    let ok = (8..=400).contains(&codigo.len())
        && simbolos >= 8
        && codigo
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        && !codigo.starts_with('-')
        && !codigo.ends_with('-');
    ok.then(|| codigo.to_owned())
}

/// Corta `texto` em `max` caracteres e troca quebras de linha por espaços.
pub fn limpar(texto: &str, max: usize) -> String {
    texto
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_do_cliente() {
        let todas = [
            DoCliente::Ola {
                nome: "Felipe".into(),
                segredo: "abc".into(),
                versao: VERSAO,
            },
            DoCliente::Adicionar { nome: "Ana".into() },
            DoCliente::Aceitar { nome: "Ana".into() },
            DoCliente::Recusar { nome: "Ana".into() },
            DoCliente::Remover { nome: "Ana".into() },
            DoCliente::Mensagem {
                para: "Ana".into(),
                texto: "oi\tcom\\tab\ne linha".into(),
            },
            DoCliente::Convite {
                para: "Ana".into(),
                codigo: "K7QM-3XPD".into(),
            },
            DoCliente::Status {
                texto: "Jogando".into(),
            },
            DoCliente::Ping,
        ];
        for m in todas {
            let l = m.codificar();
            assert!(l.ends_with('\n'));
            assert_eq!(l.matches('\n').count(), 1, "{l:?}");
            assert_eq!(DoCliente::decodificar(l.trim_end_matches('\n')), Ok(m));
        }
    }

    #[test]
    fn ida_e_volta_do_servidor() {
        let todas = [
            DoServidor::BemVindo {
                nome: "Felipe".into(),
            },
            DoServidor::Erro {
                texto: "ops".into(),
            },
            DoServidor::Amigo {
                nome: "Ana".into(),
                online: true,
                status: "".into(),
            },
            DoServidor::Amigo {
                nome: "Ana".into(),
                online: false,
                status: "Ausente".into(),
            },
            DoServidor::AmigoRemovido { nome: "Ana".into() },
            DoServidor::PedidoRecebido { nome: "Ana".into() },
            DoServidor::PedidoEnviado { nome: "Ana".into() },
            DoServidor::PedidoRemovido { nome: "Ana".into() },
            DoServidor::Mensagem {
                de: "Ana".into(),
                texto: "\\n literal".into(),
            },
            DoServidor::Convite {
                de: "Ana".into(),
                codigo: "K7QM-3XPD".into(),
            },
            DoServidor::Pong,
        ];
        for m in todas {
            let l = m.codificar();
            assert_eq!(DoServidor::decodificar(l.trim_end_matches('\n')), Ok(m));
        }
    }

    #[test]
    fn linhas_ruins() {
        assert_eq!(DoCliente::decodificar(""), Err(ErroLinha::Desconhecida));
        assert_eq!(DoCliente::decodificar("XYZ"), Err(ErroLinha::Desconhecida));
        assert_eq!(DoCliente::decodificar("PING\tx"), Err(ErroLinha::Campos));
        assert_eq!(DoCliente::decodificar("MSG\tAna"), Err(ErroLinha::Campos));
        assert_eq!(
            DoCliente::decodificar("OLA\ta\tb\tx"),
            Err(ErroLinha::Campos)
        );
        assert_eq!(
            DoCliente::decodificar("STATUS\ta\\q"),
            Err(ErroLinha::Escape)
        );
        assert_eq!(
            DoCliente::decodificar("STATUS\ta\\"),
            Err(ErroLinha::Escape)
        );
        let grande = format!("STATUS\t{}", "a".repeat(LINHA_MAX));
        assert_eq!(DoCliente::decodificar(&grande), Err(ErroLinha::Grande));
        assert_eq!(
            DoServidor::decodificar("AMIGO\tAna\t2\t"),
            Err(ErroLinha::Campos)
        );
        assert_eq!(DoCliente::decodificar("PING\r"), Ok(DoCliente::Ping));
    }

    #[test]
    fn nomes() {
        assert!(nome_valido("Felipe_08"));
        assert!(!nome_valido("ab"));
        assert!(!nome_valido("a".repeat(17).as_str()));
        assert!(!nome_valido("com espaço"));
        assert!(!nome_valido("ação"));
    }

    #[test]
    fn codigos() {
        assert_eq!(codigo_convite(" K7QM-3XPD \n"), Some("K7QM-3XPD".into()));
        assert_eq!(codigo_convite("friend:K7QM-3XPD"), Some("K7QM-3XPD".into()));
        assert_eq!(codigo_convite("olá mundo"), None);
        assert_eq!(codigo_convite("K7QM"), None);
        assert_eq!(codigo_convite("--------"), None);
        assert_eq!(codigo_convite("https://exemplo.com"), None);
    }

    #[test]
    fn limpeza() {
        assert_eq!(limpar("  oi\nvc\t ", 10), "oi vc");
        assert_eq!(limpar("abcdef", 3), "abc");
    }
}
