<h1 align="center">
wttrbar-hardened
</h1>

<p align="center">
<strong>Fork reforçado do wttrbar</strong> — Indicador climático seguro e estável para <a href="https://github.com/Alexays/Waybar/">Waybar</a> usando <a href="https://wttr.in/">wttr.in</a>.
</p>

<p align="center">
<img src="https://user-images.githubusercontent.com/55081/232401699-b8345fe0-ffce-4353-b51b-615389153448.png" height="400" alt="Exemplo do wttrbar no Waybar">
</p>

<p align="center">
<a href="#-principais-melhorias">Melhorias</a> •
<a href="#-instalação">Instalação</a> •
<a href="#-uso">Uso</a> •
<a href="#-configuração-waybar">Configuração</a> •
<a href="#-comparativo">Comparativo</a>
</p>

---
🇧🇷 Nota: Este README está em português brasileiro intencionalmente. O código é universal, mas a documentação reflete nossa comunidade. English speakers: Feel free to use your browser's translator or contribute a translation via PR!

## 🔒 Principais Melhorias

Esta versão foi completamente reescrita e auditada para resolver vulnerabilidades críticas e problemas de estabilidade da versão original.

### 🛡️ Segurança Reforçada
- **Prevenção de Path Traversal:** Validação rigorosa de entrada e uso de hash seguro para nomes de arquivos de cache, impedindo escrita arbitrária no sistema.
- **Proteção contra XSS:** Todos os dados vindos da API são escapados (HTML entity encoding) antes de serem exibidos no tooltip, prevenindo injeção de scripts.
- **Validação de Entrada:** Sanitização completa de parâmetros de localização e argumentos de linha de comando.
- **Permissões Restritas:** Arquivos de cache criados com permissão `0o600` (apenas leitura/escrita pelo dono).

### 🚀 Estabilidade e Robustez
- **Zero Panics:** Substituição de todos os `unwrap()` por tratamento de erro adequado (`Result`), garantindo que o programa nunca trave o Waybar.
- **Timeout de Rede:** Cliente HTTP configurado com timeout de 10s para evitar travamentos em conexões lentas.
- **Retry Exponencial:** Lógica inteligente de重试 com backoff exponencial para falhas temporárias de rede.
- **Fallbacks Seguros:** Valores padrão seguros quando dados da API estão ausentes ou malformados.

### ⚡ Performance Otimizada
- **Busca O(1):** Uso de `HashMap` com inicialização lazy (`OnceLock`) para ícones climáticos, eliminando buscas lineares O(n).
- **Pré-alocação de Memória:** Strings de tooltip pré-alocadas para reduzir realocações dinâmicas.
- **Sem Clones Desnecessários:** Uso eficiente de referências e slices borrowed.

### 🏗️ Qualidade de Código
- **Arquitetura Modular:** Código organizado em funções especializadas (< 30 linhas cada) seguindo princípios SOLID.
- **Documentação Completa:** Doc comments em todas as funções públicas e módulos.
- **Testes Unitários:** Suite de testes abrangente cobrindo validação, formatação e segurança.
- **Tipagem Forte:** Uso rigoroso do sistema de tipos do Rust para prevenir erros em tempo de compilação.

---

## 📦 Instalação

### Compilação Manual (Recomendado)

```bash
git clone https://github.com/luckawalkerr/wttrbar-hardened.git
cd wttrbar-hardened
cargo build --release
sudo cp target/release/wttrbar /usr/local/bin/
