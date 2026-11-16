# Essencial para Horizon

Um mod para o [Horizon](https://github.com/horizon-rust/horizon) (o Minecraft: Java Edition reescrito em Rust)
com as funções que mais se usa no Essential: **zoom**, **HUD de informações** e **amigos com mensagens e
convites para o seu mundo**.

> Este projeto foi escrito do zero. Ele **não usa nem copia o código do Essential** (a licença do Essential
> não permite modificar nem fazer obras derivadas) e não se conecta aos servidores da empresa do Essential.
> "Essential" é marca da ModCore Inc.; este projeto não tem ligação com ela.

## Duas versões do mod

O Horizon mudou o formato dos mods nativos entre o launcher que está publicado e o código mais novo dele. Este
repositório tem as duas; o launcher instala sozinho a que ele entende.

| Pasta | Para qual Horizon | Formato |
| --- | --- | --- |
| `essencial/` | **launcher 0.2.x (o publicado hoje)**, build Nightly de 13/11/2026 | `horizon-mixin.toml` + pasta `api/` |
| `mod/` | o código mais novo do Horizon (launcher que ainda não saiu) | `horizon-mod.toml` |

As duas fazem as mesmas coisas; muda só onde ficam as opções e os comandos (veja abaixo).

## O que tem

| Função | Como usar |
| --- | --- |
| **Zoom** | Aperte **C**. A roda do mouse aproxima e afasta (de 1,5x a 50x). |
| **HUD** | FPS, coordenadas e direção num canto da tela (e o nível do zoom enquanto ele está ativo). |
| **Amigos** | A tecla **O** abre a lista: adicionar, aceitar, mandar mensagem, convidar e remover. |
| **Avisos** | Mensagens, convites e pedidos de amizade aparecem como notificação no canto da tela. |
| **Convites** | Manda o código do "Jogue com amigos" do Horizon para um amigo com um clique. |

**Opções** ficam em **Opções > Mods > Essencial**: zoom inicial, apertar ou segurar a tecla do zoom, roda do
mouse no zoom, HUD e o canto dele, amigos ligados, mostrar aos amigos em que servidor você está, e os avisos.
As teclas (C e O) também mudam nessa página.

Se você já tem o mod **Zoom** do Horizon instalado, ele usa a tecla Z e o Essencial a C; os dois funcionam, mas
basta um.

## Instalar

Você precisa do launcher do Horizon com uma instância **já preparada**: se ela aparece como "Precisa ser
preparada", clique em **Jogar** nela uma vez antes (o launcher monta o jogo a partir do seu Minecraft 26.3).

1. No launcher, abra a página **Mods**.
2. Em **Adicionar um mod nativo do GitHub**, cole
   `https://github.com/felipemartins082006-cmyk/essencial-horizon` e clique em **Adicionar do GitHub**.
3. Aceite o aviso de código nativo. Mods nativos rodam com acesso ao seu computador, então só instale o que você
   confia.
4. Em **Instalado**, clique em **Compilar** (a primeira vez demora alguns minutos) e depois abra o jogo.

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

### Rodar o servidor (Windows, sem instalar nada)

1. Baixe o servidor:
   [essencial-servidor-windows.exe](https://github.com/felipemartins082006-cmyk/essencial-horizon/releases/download/servidor/essencial-servidor-windows.exe)
   (também na página **Releases** do repositório, em "Servidor de amigos").
2. Crie uma pasta, por exemplo `Documentos\Essencial Servidor`, e coloque o arquivo nela. É ali que ele guarda as
   contas (`essencial-dados.json`).
3. Dê dois cliques no arquivo.
   - Se o Windows mostrar "O Windows protegeu o computador", clique em **Mais informações** e depois em
     **Executar assim mesmo** (o arquivo não é assinado, por isso o aviso).
   - Se o Firewall perguntar, marque **Redes privadas** e clique em **Permitir acesso**.
4. Abre uma janela preta dizendo **Servidor do Essencial ouvindo em 0.0.0.0:25590** e mostrando o que cada jogador
   coloca na configuração. **Deixe essa janela aberta** enquanto jogam; para desligar, feche a janela.

Que endereço usar:

- **Você mesmo, no computador do servidor:** `127.0.0.1:25590`.
- **Amigos na mesma casa (mesmo Wi-Fi):** o IP que a janela mostra em "na mesma rede", por exemplo `192.168.0.10:25590`.
- **Amigos em outras casas:** o jeito mais fácil é o [Tailscale](https://tailscale.com) (grátis): todos instalam,
  entram na mesma rede do Tailscale, e usam o IP `100.x.x.x` que o Tailscale mostra para o computador do
  servidor, com `:25590` no fim. A outra opção é abrir a porta 25590 (TCP) no roteador e usar o seu IP público.

No Linux ou macOS, rode pelo código (precisa do [Rust](https://rustup.rs)):

```bash
cd servidor
```

```bash
cargo run --release
```

Para mudar a porta ou onde ficam os dados: `cargo run --release -- --porta 25590 --dados /caminho/essencial-dados.json`
(no Windows, os mesmos `--porta` e `--dados` depois do `.exe`, por um atalho ou pelo Prompt de Comando).

### Ligar os amigos no mod

Depois de abrir o jogo com o mod uma vez, edite o arquivo `config/essencial.toml` da instância (a pasta da
instância abre pelo launcher):

```toml
servidor = "192.168.0.10:25590"
```

O mod percebe a mudança sozinho, sem reiniciar. Ele usa o nome da conta conectada no launcher; se você joga sem
conta, coloque também `nome = "SeuNome"` no mesmo arquivo. Depois aperte **O** num mundo.

### Como a conta funciona

Não tem senha nem cadastro. Na primeira vez que você conecta, o servidor guarda o seu nome de jogador junto com
um segredo aleatório que o mod cria neste computador (`segredo.txt`, na pasta de dados do mod). Depois, só esse
computador entra com esse nome. Se você trocar de computador, copie o `segredo.txt` para o novo.

## Comandos do chat

Na versão `essencial/` (launcher 0.2.x):

| Comando | O que faz |
| --- | --- |
| `/amigos` | abre a tela de amigos |
| `/e add <nome>` | pede amizade |
| `/e aceitar <nome>` / `/e recusar <nome>` | responde a um pedido |
| `/e remover <nome>` | desfaz a amizade (ou cancela um pedido que você fez) |
| `/e msg <nome> <texto>` | mensagem privada |
| `/e r <texto>` | responde a última mensagem |
| `/e convidar <nome>` | manda o código do seu mundo |
| `/e entrar` | copia o código do último convite que você recebeu |
| `/e lista` | mostra quem está online |
| `/e ajuda` | lista os comandos na tela de amigos |

Esses comandos ficam no seu jogo; nunca vão para o servidor em que você está jogando. Na versão `mod/` os
mesmos comandos começam com ponto (`.add`, `.msg`, `.convidar`...).

### Convidar para o seu mundo

1. No seu mundo, aperte **Esc** e clique no botão **Amigos** (Jogue com amigos).
2. Clique em **Comece a compartilhar** e depois em **Copiar código**.
3. Na tela do Essencial (**O**), clique em **Convidar** ao lado do amigo, ou digite `/e convidar <nome>`.

O amigo recebe uma notificação. Ele aperta **O** e clica em **Copiar convite** (ou digita `/e entrar`) e cola o
código em **Multijogador > Conexão direta**, ou no botão **Amigos** da tela de título (**Junte-se a um amigo**).
A API de mods do Horizon não deixa um mod entrar num mundo sozinho; por isso é preciso colar o código.

## Limitações

- As mensagens entre o mod e o servidor de amigos **não são criptografadas**. Não mande nada sensível por elas,
  e prefira rede local ou VPN. (O mundo compartilhado pelo Horizon continua criptografado.)
- Mensagens só chegam se o amigo estiver online; não ficam guardadas.
- Na versão `essencial/`, as mensagens aparecem como notificação e na tela de amigos, não no chat do jogo (a API
  de mods dessa versão não deixa um mod escrever no chat), e o HUD não mostra o ping.
- Não tem cosméticos, emotes nem gerenciador de capturas de tela.
- `essencial/` é fixado no build do Horizon de 13/11/2026. Se o seu launcher estiver em outro build, ele pergunta
  se quer **Compilar mesmo assim**; normalmente funciona.

## Para quem quer mexer no código

```text
essencial/              o mod para o launcher 0.2.x
  horizon-mixin.toml    manifesto: id, versão e o build do Horizon fixado
  api/src/lib.rs        os ganchos do jogo (zoom, HUD, teclas, comandos, tela)
  api/src/amigos.rs     lista de amigos, avisos e comandos /e
  api/src/tela.rs       a tela de amigos
  api/src/opcoes.rs     as opções e o config/essencial.toml
mod/                    o mod para o Horizon mais novo (horizon-mod.toml)
servidor/               o servidor de amigos (Rust puro, sem o Horizon)
```

`protocolo.rs` (o protocolo entre mod e servidor) e `rede.rs` (a conexão) são o mesmo arquivo nas duas versões do
mod; o servidor usa o de `mod/`, e o CI confere que as cópias de `essencial/` são iguais.

Testes do servidor, que também rodam a lógica do mod (lista de amigos, comandos, tela, opções) e um teste de
ponta a ponta com o código de rede do próprio mod:

```bash
cd servidor && cargo test
```

## Licença

MIT. Veja [LICENSE](LICENSE).
