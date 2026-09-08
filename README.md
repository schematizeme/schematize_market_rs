# schematize market

Instala e remove programas. Só isso, e bem.

```
schematize-market list                          # tudo que dá pra instalar, e o que já está
schematize-market install go --method mise
schematize-market install schematize-deployer
schematize-market switch rust --to official
schematize-market desktop --install             # ícone no menu de aplicativos
```

## O que ele instala

| | |
|---|---|
| **runtimes** | Go, Rust, Node, Python, Ruby, C#… por `docker`, `mise`, `distro` ou `official` |
| **ferramentas de dev** | Claude Code, VS Code, Codex CLI |
| **apps do ecossistema** | `schematize-deployer`, `schematize-optimizer`, `schematize-skills`, `schematize-overdev` |

**Uma lista só.** O pedido que originou este app foi *"deployer e optimizer deveriam aparecer
no env para instalar"*. A leitura rasa seria somar os apps à tabela do `env`. A leitura certa
foi ver que `env` (linguagens) e `apps` (apps da casa) eram o **mesmo produto** partido em dois
comandos do hub — e promover os dois juntos a app próprio (ADR-0012).

## Ele diz por onde a coisa entrou

Um runtime instalado não vira um "instalado" mudo. O market pergunta ao gerenciador de pacotes
quem é dono do arquivo (`rpm -qf`, `dpkg -S`) e mostra a procedência:

```
  Rust           docker, mise, distro, official     via distro (cargo1.97)
  Node.js        docker, mise, distro, official     via mise
```

É o que torna o `switch` possível: sem saber por onde está instalado, trocar de método é
adivinhação. O `switch` **instala o novo antes de remover o velho**, e **recusa** quando a
procedência é desconhecida — desinstalar o que não se sabe de onde veio é como se perde uma
máquina.

## Distros

Compatibilidade de primeira classe com **Debian/Ubuntu, Fedora, SUSE e Arch** (e derivados),
detectadas por `/etc/os-release`. Quando um pacote não está nos repositórios padrão daquela
família, o market **diz isso e pergunta** em vez de inventar um repositório de terceiro — não
há tentativa de prever 999 distros, há honestidade sobre o que se sabe.

## Independência

Abre e funciona sozinho, sem o hub instalado (piso 10). Tem binário próprio, ícone próprio e
entrada própria no menu de aplicativos. Quando o hub está presente, os dois se enxergam.

## Desenvolvimento

```
cargo test --all-targets      # 58 testes
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

A superfície da CLI é contrato: `tests/superficie-cli.txt` reprova qualquer mudança acidental
em comando ou flag. Se a mudança for intencional,
`MARKET_REGRAVA_SUPERFICIE=1 cargo test superficie_da_cli` e **leia o diff linha por linha**.

## Licença

MIT — ver [LICENSE](LICENSE).
