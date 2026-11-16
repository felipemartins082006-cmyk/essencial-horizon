# Essencial para Horizon

Um mod para o [Horizon](https://github.com/horizon-rust/horizon) (o Minecraft: Java Edition reescrito em Rust)
com as funções que mais se usa no Essential: **zoom**, **HUD de informações** e **amigos com chat e convites
para o seu mundo**.

> Este projeto foi escrito do zero. Ele **não usa nem copia o código do Essential** (a licença do Essential
> não permite modificar nem fazer obras derivadas) e não se conecta aos servidores da empresa do Essential.
> "Essential" é marca da ModCore Inc.; este projeto não tem ligação com ela.

## O que tem

| Função | Como usar |
| --- | --- |
| **Zoom** | Segure **C**. A roda do mouse aproxima e afasta (de 1,5x a 50x). |
| **HUD** | FPS, coordenadas, direção e ping num canto da tela. |
| **Amigos** | A tecla **O** abre a lista: adicionar, aceitar, mandar mensagem, convidar e remover. |
| **Chat** | Comandos que começam com `.` (veja abaixo); eles não vão para o servidor do jogo. |
| **Convites** | Manda o código do "Jogue com amigos" do Horizon para um amigo com um clique. |

As teclas mudam em **Opções > Controles** (categoria Essencial). O resto fica em **Opções > Mods > Essencial >
Config**: nível inicial do zoom, zoom por toque (liga/desliga) ou segurando, quais linhas o HUD mostra e em
qual canto, mostrar ou não aos amigos em que servidor você está, e os avisos na tela.

## Instalar o mod

Você precisa do launcher do Horizon com uma instância já criada.

1. No launcher, abra a página **Mods**.
2. Clique em **Adicionar do GitHub** e cole:
   `https://github.com/felipemartins082006-cmyk/essencial-horizon`
3. Aceite o aviso de código nativo. O launcher mostra o link do código antes; mods nativos rodam com acesso
   ao seu computador, então só instale o que você confia.
4. O launcher compila o mod (na primeira vez demora alguns minutos). Depois é só abrir o jogo.

Pelo terminal é o mesmo:

```bash
horizon-launcher mods add-github https://github.com/felipemartins082006-cmyk/essencial-horizon --trust-native-code
```

O zoom e o HUD já funcionam assim. Para os **amigos**, falta dizer ao mod qual servidor usar (próxima seção).

## Amigos: o servidor

A lista de amigos precisa de um servidor pequeno para saber quem é amigo de quem, quem está online e para
passar as mensagens. Ele **não carrega o jogo**: quando você convida alguém, o mundo vai direto do seu
computador para o do amigo pelo "Jogue com amigos" do próprio Horizon. O servidor só repassa o código do convite.

Uma pessoa do grupo roda o servidor; todos colocam o endereço dele no mod.

### Rodar o servidor

Precisa do [Rust](https://rustup.rs) instalado. Na pasta deste repositório:

```bash
cd servidor
```

```bash
cargo run --release
```

Ele ouve na porta **25590** e guarda as contas em `essencial-dados.json`, na pasta onde foi aberto. Para mudar:

```bash
cargo run --release -- --porta 25590 --dados /caminho/para/essencial-dados.json
```

- **Mesma casa (rede local):** use o IP do computador do servidor, por exemplo `192.168.0.10:25590`.
- **Pela internet:** rode numa VPS, ou abra a porta 25590 (TCP) no roteador para o computador do servidor.
  Uma VPN como Tailscale ou ZeroTier também funciona e evita abrir porta.

### Ligar os amigos no mod

Depois de abrir o jogo com o mod uma vez, edite o arquivo `config/essencial.toml` da instância:

```toml
servidor = "192.168.0.10:25590"
```

O mod percebe a mudança sozinho, sem reiniciar. Entre num mundo ou servidor (o mod precisa do seu nome de
jogador) e aperte **O**.

### Como a conta funciona

Não tem senha nem cadastro. Na primeira vez que você conecta, o servidor guarda o seu nome de jogador junto com
um segredo aleatório que o mod cria neste computador (em `horizon-mod-data/essencial/segredo.txt`). Depois, só
esse computador entra com esse nome. Se você trocar de computador, copie o `segredo.txt` para o novo.

## Comandos do chat

| Comando | O que faz |
| --- | --- |
| `.amigos` | abre a tela de amigos |
| `.add <nome>` | pede amizade |
| `.aceitar <nome>` / `.recusar <nome>` | responde a um pedido |
| `.remover <nome>` | desfaz a amizade (ou cancela um pedido que você fez) |
| `.msg <nome> <texto>` | mensagem privada |
| `.r <texto>` | responde a última mensagem |
| `.convidar <nome>` | manda o código do seu mundo |
| `.entrar` | copia o código do último convite que você recebeu |
| `.lista` | mostra quem está online |
| `.ajuda` | lista os comandos |

### Convidar para o seu mundo

1. No seu mundo, aperte **Esc** e clique no botão **Amigos** (Jogue com amigos).
2. Clique em **Comece a compartilhar** e depois em **Copiar código**.
3. Na tela do Essencial (**O**), clique em **Convidar** ao lado do amigo, ou digite `.convidar <nome>`.

O amigo recebe uma mensagem com **[Copiar código]**. Ele cola o código em **Multijogador > Conexão direta**, ou no
botão **Amigos** da tela de título (**Junte-se a um amigo**). A API de mods do Horizon não deixa um mod entrar num
mundo sozinho; por isso é preciso colar o código.

## Limitações

- As mensagens entre o mod e o servidor de amigos **não são criptografadas**. Não mande nada sensível por elas,
  e prefira rede local ou VPN. (O mundo compartilhado pelo Horizon continua criptografado.)
- Mensagens só chegam se o amigo estiver online; não ficam guardadas.
- Não tem cosméticos, emotes nem gerenciador de capturas de tela.
- O mod foi feito para a versão do Horizon fixada em `mod/horizon-mod.toml` (`[native] horizon`). O launcher
  compila contra a versão que você tem instalada, se ela for compatível. Se ele recusar, rode
  `cargo horizon-mod update` na pasta `mod/` para fixar a versão nova.

## Para quem quer mexer no código

```text
mod/                    o mod (projeto nativo do Horizon)
  horizon-mod.toml      manifesto: id, versão e a versão do Horizon fixada
  src/lib.rs            os ganchos (hooks) do jogo
  src/zoom.rs           zoom
  src/hud.rs            HUD
  src/amigos/           lista de amigos, comandos do chat, tela (tela.rs) e rede (rede.rs)
  src/protocolo.rs      o protocolo de texto entre mod e servidor (o servidor usa o mesmo arquivo)
  assets/essencial/lang traduções (pt_br e en_us)
servidor/               o servidor de amigos (Rust puro, sem o Horizon)
```

Testes do servidor, que incluem um teste de ponta a ponta usando o código de rede do próprio mod:

```bash
cd servidor && cargo test
```

Os testes do mod precisam do código do Horizon. Clone o Horizon e aponte `mod/.horizon/sdk/api` para ele (o
`cargo horizon-mod` faz isso sozinho quando há SDK publicado para a versão fixada):

```bash
git clone https://github.com/horizon-rust/horizon ../horizon
```

```bash
mkdir -p mod/.horizon/sdk && ln -s "$(pwd)/../horizon" mod/.horizon/sdk/api
```

```bash
cd mod && cargo test
```

## Licença

MIT. Veja [LICENSE](LICENSE).
