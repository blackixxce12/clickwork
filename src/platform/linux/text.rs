//! Words for a Linux desktop.
//!
//! The interface strings and the handbook were written for the Windows build and
//! say so in places: `Windows.Media.Ocr`, `Win+Shift+S`, `.exe`, Mica. None of
//! that exists here, and a Linux program that talks about Windows reads as an
//! afterthought. Rather than fork three hundred articles, this module rewrites
//! the sentences that name the other platform, once, when a table or an article
//! is first asked for. A test walks every article in every language and fails on
//! any Windows word left behind, so a new article cannot slip one in.

use crate::Lang;
use crate::handbook::{Article, Topic};
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

/// Interface strings replaced per language table (indices as in `tables()`).
pub fn strings(idx: usize) -> BTreeMap<String, String> {
    let (exe, ahk, ocr) = match idx {
        1 => ("⚙ Экспорт плеера", "📜 Экспорт", "языки системы"),
        2 => ("⚙ Експорт плеєра", "📜 Експорт", "мови системи"),
        3 => ("⚙ Exportar reprodutor", "📜 Exportar", "idiomas do sistema"),
        4 => ("⚙ Exportar reproductor", "📜 Exportar", "idiomas del sistema"),
        5 => ("⚙ 导出播放器", "📜 导出", "系统语言"),
        _ => ("⚙ Export player", "📜 Export", "the system languages"),
    };
    let mut m = BTreeMap::new();
    m.insert("export_exe".into(), exe.into());
    m.insert("export_ahk".into(), ahk.into());
    m.insert("ocr_lang_auto".into(), ocr.into());
    m
}

/// Words that have no business in a Linux program. The test looks for these.
pub const FORBIDDEN: &[&str] = &[
    "Windows",
    "Win+",
    ".exe",
    "AutoHotkey",
    ".ahk",
    "UI Automation",
    "Mica",
    "Acrylic",
    "SendInput",
    "Task Scheduler",
    "Планировщик заданий",
    "Планувальник завдань",
    "Agendador de Tarefas",
    "Programador de tareas",
    "任务计划程序",
    "%APPDATA%",
    "Media Foundation",
    "Desktop Duplication",
    "DirectX",
];

/// Article titles that change.
const TITLES: &[(&str, Lang, &str)] = &[
    ("uia", Lang::En, "Interface elements"),
    ("uia", Lang::Ru, "Элементы интерфейса"),
    ("uia", Lang::Uk, "Елементи інтерфейсу"),
    ("uia", Lang::Pt, "Elementos da interface"),
    ("uia", Lang::Es, "Elementos de la interfaz"),
    ("uia", Lang::Zh, "界面元素"),
];

/// Articles replaced whole: the export one describes two Windows formats.
const BODIES: &[(&str, Lang, &str)] = &[
    ("export", Lang::En, r#"
# A standalone player
**Export player** produces a self-running copy of this program with the macro inside it, for another Linux machine with nothing installed. Scripts are included, not just recordings.
It works by copying this executable and appending the macro to it: an ELF image ignores trailing bytes, which is the same trick self-extracting archives use. No compiler or linker is involved, and the output is about sixteen megabytes because the player *is* the whole program.
The four choices it carries — repeat count, speed, absolute mouse, delay between loops — are fixed when you build it. Everything else it reads from a settings file beside it if somebody put one there, and uses defaults otherwise.
> The export carries the macro. It does not carry the pictures. For a macro that looks at the screen, use **Macro package**.
"#),
    ("export", Lang::Ru, r#"
# Отдельный плеер
**Экспорт плеера** создаёт самостоятельный плеер — копию программы с макросом внутри — для другой машины с Linux без отдельной установки Clickwork. В экспорт можно включать и скрипты, а не только обычные записи.
Экспорт копирует текущий исполняемый файл и дописывает макрос в его конец. ELF-файл допускает дополнительные данные после образа, поэтому отдельная компиляция не требуется. Итоговый файл содержит сам плеер и имеет размер около шестнадцати мегабайт.
При экспорте фиксируются четыре настройки: число повторов, скорость, абсолютная мышь и задержка между повторами. Остальные параметры берутся из файла настроек рядом с экспортом, если он существует, иначе используются значения по умолчанию.
> Экспорт передаёт сам макрос, но не его изображения. Для макросов, использующих шаблоны, нужен **Пакет макроса**.
"#),
    ("export", Lang::Uk, r#"# Окремий плеєр
**Експорт плеєра** робить самостійний плеєр — копію програми з макросом усередині — для іншої машини з Linux без окремо встановленого Clickwork. В експорт можна брати і скрипти, а не тільки звичайні записи.
Експорт копіює теперішній виконуваний файл і дописує макрос у його кінець. ELF-файл дозволяє додаткові дані після образу, тому окремо нічого компілювати не треба. Готовий файл містить сам плеєр і важить близько шістнадцяти мегабайт.
Під час експорту закріплюються чотири налаштування: кількість повторів, швидкість, абсолютна миша і затримка між повторами. Решту беруть із файла налаштувань поруч з експортом, якщо той є, а як немає — з типових значень.
> Експорт несе самий макрос, але не його зображення. Для макросів із шаблонами потрібен **Пакет макросу**.
"#),
    ("export", Lang::Pt, r#"# Um leitor independente
**Exportar leitor** cria um leitor autónomo — uma cópia do programa com a macro lá dentro — para outra máquina Linux sem ter o Clickwork instalado à parte. Na exportação cabem também os guiões, não só as gravações normais.
A exportação copia o executável atual e acrescenta-lhe a macro no fim. Um ficheiro ELF aceita dados extra depois da imagem, por isso não é preciso compilar nada. O ficheiro que sai leva o leitor lá dentro e ocupa uns dezasseis megabytes.
Ao exportar ficam fixadas quatro definições: número de repetições, velocidade, mouse absoluto e pausa entre repetições. O resto vem do ficheiro de definições que estiver ao lado da exportação e, se não houver, dos valores de origem.
> A exportação leva a macro, mas não as imagens dela. Para macros que usam modelos é preciso um **Pacote de macro**.
"#),
    ("export", Lang::Es, r#"# Un reproductor independiente
**Exportar reproductor** crea un reproductor autónomo — una copia del programa con la macro dentro — para otra máquina Linux sin instalar Clickwork aparte. En la exportación caben también los guiones, no solo las grabaciones normales.
La exportación copia el ejecutable actual y le añade la macro al final. Un archivo ELF admite datos extra después de la imagen, así que no hay que compilar nada. El archivo resultante lleva dentro el reproductor y ocupa unos dieciséis megabytes.
Al exportar quedan fijados cuatro ajustes: número de repeticiones, velocidad, ratón absoluto y pausa entre repeticiones. El resto sale del archivo de ajustes que haya junto a la exportación, y si no lo hay, de los valores de fábrica.
> La exportación se lleva la macro, pero no sus imágenes. Para macros que usan plantillas hace falta un **Paquete de macro**.
"#),
    ("export", Lang::Zh, r#"# 独立的播放器
**导出播放器** 会做出一个独立的播放器——带着宏的程序副本——不必另外安装 Clickwork 就能在另一台 Linux 机器上运行。导出的内容也可以包含脚本，不限于普通录制。
导出的做法是把当前的可执行文件复制一份，再把宏追加到它末尾。ELF 文件允许映像之后带额外数据，所以不需要单独编译。做出来的文件自带播放器，大约十六兆字节。
导出时有四项设置被固定下来：重复次数、速度、绝对鼠标和重复之间的间隔。其余参数取自导出文件旁边的设置文件；没有那个文件就用默认值。
> 导出带走的只是宏本身，不含它的图片。用到模板的宏需要 **宏包**。
"#),
];

/// Sentences replaced in one article. Applied before the phrase table, so a
/// sentence can be rewritten whole where a word swap would read badly.
const SPECIFIC: &[(&str, Lang, &[(&str, &str)])] = &[
    // ---- English -----------------------------------------------------------
    ("first-run", Lang::En, &[("ask Windows for a control by its name", "ask the desktop's accessibility tree for a control by its name")]),
    ("three-ways", Lang::En, &[
        ("\"Windows, give me the button called Claim.\"", "\"Desktop, give me the button called Claim.\""),
        ("It only works in interfaces that Windows knows about — ordinary applications, browsers, most desktop software. Games draw their own interfaces and are silent to it. Roblox, anything in Unity or DirectX: nothing.",
         "It only works in interfaces that describe themselves on the accessibility bus — GTK, Qt, most desktop software, browsers. Games draw their own interfaces and are silent to it. Roblox, anything in Unity or Vulkan: nothing."),
    ]),
    ("hotkeys", Lang::En, &[(
        "Windows lets only one program own a hotkey at a time. If another one already has it, the log says which of yours failed to register and the field shows it. Pick something else, or close the other program.",
        "On Hyprland each hotkey is also a compositor keybind, which is what keeps the key from the application in front. A combo you already bind yourself is left to you: the log says so, and that hotkey then comes from the input devices instead, where the key also reaches the application. Pick something else, or free the combo.",
    )]),
    ("test-run", Lang::En, &[("that would have reached Windows with a click or a keystroke", "that would have sent a click or a keystroke to the compositor")]),
    ("analyze", Lang::En, &[("an element if Windows knew about the control", "an element if the accessibility tree knew about the control")]),
    ("vision", Lang::En, &[("snip with `Win+Shift+S` and press **Paste**", "snip with your screenshot tool (`grim -g \"$(slurp)\" - | wl-copy` does it from a terminal) and press **Paste**")]),
    ("ocr", Lang::En, &[
        ("using the recognition already built into Windows. No model is shipped and nothing leaves the machine.",
         "with Tesseract, opened from the system at run time. No model is shipped and nothing leaves the machine; the languages are the `tesseract-data-xxx` packages installed."),
        ("Empty means \"the Windows display languages\", which is right until the program you are automating is in a different language from Windows.",
         "Empty means \"the desktop's language plus English\", which is right until the program you are automating is in a language you have no pack for."),
        ("on a Russian Windows reading an English game", "on a Russian desktop reading an English game"),
        ("Setting the language to `en-US` on the same machine", "Setting the language to `eng` on the same machine"),
        ("Only recognisers Windows actually has installed are offered", "Only the language packs actually installed are offered"),
    ]),
    ("uia", Lang::En, &[
        ("Asks Windows for a control by name instead of looking at pixels.", "Asks the accessibility bus (AT-SPI2) for a control by name instead of looking at pixels."),
        ("be redrawn by a Windows update", "be redrawn by a toolkit update"),
        ("Games in Unity or DirectX, Roblox", "Games in Unity or Vulkan, Roblox"),
    ]),
    ("targets", Lang::En, &[("the element Windows calls Claim", "the element the accessibility tree calls Claim")]),
    ("schedule", Lang::En, &[("Windows Task Scheduler pointed at `--play file --no-gui` is the better tool", "a systemd timer or a cron job pointed at `--play file --no-gui` is the better tool")]),
    ("why", Lang::En, &[("A UI Automation rung that costs 900 ms", "An accessibility rung that costs 900 ms")]),
    ("files", Lang::En, &[(
        "Everything lives beside the executable if that folder is writable, and in `%APPDATA%\\Clickwork\\` if it is not — which is what happens under `C:\\Program Files`.",
        "Everything lives in `~/.config/clickwork` (or `$XDG_CONFIG_HOME/clickwork`); set `CLICKWORK_PORTABLE=1` to keep it beside the executable instead.",
    )]),
    ("appearance", Lang::En, &[(
        "Nine of them, including a Fluent one that uses Windows 11's own Mica or Acrylic backdrop, so the window picks up the desktop behind it the way native applications do.",
        "Nine of them, including a translucent Fluent one and a Glass one: the window draws its own translucency, and the compositor adds the blur behind it the way Hyprland does.",
    )]),
    ("screen-recording", Lang::En, &[(
        "Encoding goes through Media Foundation, which is the part of Windows that already knows about the graphics card's encoder. On a machine with an NVIDIA, AMD or Intel GPU it uses that; on one without, it falls back to the processor. There is no separate download and no third-party component.",
        "Encoding is done by `gpu-screen-recorder` or `wf-recorder`, whichever is installed: the first uses the graphics card's encoder (NVIDIA, AMD or Intel), the second the processor. Without either the panel says so.",
    )]),
    ("cli", Lang::En, &[
        ("A macro in Task Scheduler runs at four in the morning", "A macro started by a systemd timer runs at four in the morning"),
        ("every `SendInput` call site is silenced for the duration", "every input-sending call is silenced for the duration"),
    ]),
    // ---- Russian -----------------------------------------------------------
    ("first-run", Lang::Ru, &[("запросить элемент у Windows", "запросить элемент у дерева доступности")]),
    ("three-ways", Lang::Ru, &[
        ("«Windows, дай мне кнопку с названием Claim»", "«Рабочий стол, дай мне кнопку с названием Claim»"),
        ("Работает там, где приложение предоставляет Windows информацию об элементах интерфейса:", "Работает там, где приложение описывает свои элементы на шине доступности (AT-SPI2):"),
    ]),
    ("test-run", Lang::Ru, &[
        ("но не отправляет реальный ввод в Windows.", "но не отправляет реальный ввод композитору."),
        ("Действия, которые должны отправить ввод в Windows,", "Действия, которые должны отправить ввод композитору,"),
    ]),
    ("vision", Lang::Ru, &[("вырезать область через `Win+Shift+S` и нажать **Вставить**", "вырезать область инструментом скриншотов (например, `grim -g \"$(slurp)\" - | wl-copy`) и нажать **Вставить**")]),
    ("ocr", Lang::Ru, &[
        ("с помощью встроенного OCR Windows. Отдельная модель не поставляется, и данные не отправляются за пределы компьютера.",
         "с помощью Tesseract, который подключается из системы во время работы. Отдельная модель не поставляется, данные не покидают компьютер; языки — установленные пакеты `tesseract-data-xxx`."),
        ("используются языки интерфейса Windows.", "используются язык рабочего стола и английский."),
        ("на русской Windows при распознавании", "на русской системе при распознавании"),
        ("выбор `en-US` позволил", "выбор `eng` позволил"),
        ("которые действительно установлены в Windows.", "которые действительно установлены в системе."),
    ]),
    ("uia", Lang::Ru, &[("Запрашивает у Windows элемент интерфейса по его свойствам,", "Запрашивает у шины доступности (AT-SPI2) элемент интерфейса по его свойствам,")]),
    ("schedule", Lang::Ru, &[("**Планировщик заданий Windows**", "**таймер systemd или cron**")]),
    ("files", Lang::Ru, &[(
        "Файлы хранятся рядом с исполняемым файлом, если каталог доступен для записи. Если нет, используется `%APPDATA%\\Clickwork\\`, например при установке в `C:\\Program Files`.",
        "Файлы хранятся в `~/.config/clickwork` (или `$XDG_CONFIG_HOME/clickwork`); `CLICKWORK_PORTABLE=1` держит их рядом с исполняемым файлом.",
    )]),
    ("appearance", Lang::Ru, &[(
        "Доступно девять тем, включая тему Fluent с использованием Mica или Acrylic Windows 11. В соответствующем режиме окно использует системный эффект прозрачности.",
        "Доступно девять тем, включая полупрозрачные Fluent и Glass: их прозрачность рисует само окно, а размытие фона добавляет композитор, как это делает Hyprland.",
    )]),
    ("screen-recording", Lang::Ru, &[(
        "Кодирует Media Foundation — часть Windows, которая умеет обращаться к кодировщику видеокарты. Есть NVIDIA, AMD или Intel — считает видеокарта; подходящей карты нет — считает процессор. Скачивать и ставить отдельно ничего не нужно.",
        "Кодирует `gpu-screen-recorder` или `wf-recorder` — что установлено: первый использует кодировщик видеокарты (NVIDIA, AMD или Intel), второй — процессор. Без них панель так и скажет.",
    )]),
    ("cli", Lang::Ru, &[
        ("через «Планировщик заданий»", "через таймер systemd"),
        ("вызовы `SendInput` на время проверки заглушены.", "отправка ввода на время проверки заглушена."),
    ]),
    // ---- Ukrainian -----------------------------------------------------------
    ("screen-recording", Lang::Uk, &[(
        "Кодує Media Foundation, частина Windows, яка вміє звертатися до кодувальника відеокарти. Є NVIDIA, AMD чи Intel — рахує відеокарта; підхожої карти немає — рахує процесор. Завантажувати і ставити окремо нічого не треба.",
        "Кодує `gpu-screen-recorder` або `wf-recorder` — що встановлено: перший використовує кодувальник відеокарти (NVIDIA, AMD чи Intel), другий — процесор. Без них панель так і скаже.",
    )]),
    ("first-run", Lang::Uk, &[("запитати елемент у Windows", "запитати елемент у дерева доступності")]),
    ("three-ways", Lang::Uk, &[
        ("«Windows, дай мені кнопку з назвою Claim»", "«Стільнице, дай мені кнопку з назвою Claim»"),
        ("Працює там, де програма повідомляє Windows про свої елементи інтерфейсу:", "Працює там, де програма описує свої елементи на шині доступності (AT-SPI2):"),
    ]),
    ("test-run", Lang::Uk, &[("але не надсилає справжнього вводу у Windows.", "але не надсилає справжнього вводу композитору.")]),
    ("vision", Lang::Uk, &[("вирізати область через `Win+Shift+S` і натиснути **Вставити**", "вирізати область інструментом знімків екрана (наприклад, `grim -g \"$(slurp)\" - | wl-copy`) і натиснути **Вставити**")]),
    ("ocr", Lang::Uk, &[
        ("вбудованим OCR Windows. Окремої моделі програма не постачає, і дані не виходять за межі комп'ютера.",
         "за допомогою Tesseract, який підключається з системи під час роботи. Окремої моделі програма не постачає, дані не виходять за межі комп'ютера; мови — встановлені пакети `tesseract-data-xxx`."),
        ("беруться мови інтерфейсу Windows.", "беруться мова стільниці та англійська."),
        ("на російській Windows під час розпізнавання", "на російській системі під час розпізнавання"),
        ("вибір `en-US` дав", "вибір `eng` дав"),
        ("які справді встановлено у Windows.", "які справді встановлено в системі."),
    ]),
    ("uia", Lang::Uk, &[("Запитує в Windows елемент інтерфейсу за його властивостями,", "Запитує в шини доступності (AT-SPI2) елемент інтерфейсу за його властивостями,")]),
    ("schedule", Lang::Uk, &[("**Планувальник завдань Windows**", "**таймер systemd або cron**")]),
    ("files", Lang::Uk, &[(
        "Файли лежать поруч із виконуваним файлом, якщо в той каталог можна писати. Якщо ні, береться `%APPDATA%\\Clickwork\\`, скажімо коли програму поставлено в `C:\\Program Files`.",
        "Файли лежать у `~/.config/clickwork` (або `$XDG_CONFIG_HOME/clickwork`); `CLICKWORK_PORTABLE=1` тримає їх поруч із виконуваним файлом.",
    )]),
    ("appearance", Lang::Uk, &[(
        "Є дев'ять тем, разом із темою Fluent, що бере Mica або Acrylic із Windows 11. У відповідному режимі вікно користується системним ефектом прозорості.",
        "Є дев'ять тем, разом із напівпрозорими Fluent і Glass: їхню прозорість малює саме вікно, а розмиття тла додає композитор, як це робить Hyprland.",
    )]),
    ("cli", Lang::Uk, &[
        ("виклики `SendInput` на час перевірки заглушено.", "надсилання вводу на час перевірки заглушено."),
        ("через «Планувальник завдань»", "через таймер systemd"),
    ]),
    // ---- Portuguese ----------------------------------------------------------
    ("screen-recording", Lang::Pt, &[(
        "Quem codifica é o Media Foundation, a parte do Windows que sabe falar com o codificador da placa gráfica. Se houver uma NVIDIA, AMD ou Intel, calcula a placa; se não houver nenhuma que sirva, calcula o processador. Não há nada para descarregar nem instalar à parte.",
        "Quem codifica é o `gpu-screen-recorder` ou o `wf-recorder`, o que estiver instalado: o primeiro usa o codificador da placa gráfica (NVIDIA, AMD ou Intel), o segundo o processador. Sem nenhum deles o painel di-lo.",
    )]),
    ("first-run", Lang::Pt, &[("pedir o elemento ao Windows", "pedir o elemento à árvore de acessibilidade")]),
    ("three-ways", Lang::Pt, &[
        ("«Windows, dá-me o botão que se chama Claim»", "«Ambiente, dá-me o botão que se chama Claim»"),
        ("Funciona onde a aplicação diz ao Windows que elementos tem a sua interface:", "Funciona onde a aplicação descreve os seus elementos no barramento de acessibilidade (AT-SPI2):"),
    ]),
    ("test-run", Lang::Pt, &[("mas não envia entrada verdadeira para o Windows.", "mas não envia entrada verdadeira ao compositor.")]),
    ("vision", Lang::Pt, &[("recortar um pedaço com `Win+Shift+S` e premir **Colar**", "recortar um pedaço com a sua ferramenta de captura (por exemplo `grim -g \"$(slurp)\" - | wl-copy`) e premir **Colar**")]),
    ("ocr", Lang::Pt, &[
        ("com o OCR que vem no Windows. Não é distribuído nenhum modelo à parte e os dados não saem do computador.",
         "com o Tesseract, aberto a partir do sistema em tempo de execução. Não é distribuído nenhum modelo à parte, os dados não saem do computador e os idiomas são os pacotes `tesseract-data-xxx` instalados."),
        ("usam-se os idiomas da interface do Windows.", "usa-se o idioma do ambiente mais o inglês."),
        ("num Windows em russo a ler", "num sistema em russo a ler"),
        ("escolher `en-US` bastou", "escolher `eng` bastou"),
        ("que estão mesmo instalados no Windows.", "que estão mesmo instalados no sistema."),
    ]),
    ("uia", Lang::Pt, &[("Pede ao Windows um elemento de interface pelas propriedades dele,", "Pede ao barramento de acessibilidade (AT-SPI2) um elemento de interface pelas propriedades dele,")]),
    ("schedule", Lang::Pt, &[("**Agendador de Tarefas do Windows**", "**temporizador do systemd ou cron**")]),
    ("files", Lang::Pt, &[(
        "Os ficheiros vivem ao lado do executável, desde que nessa pasta se possa escrever. Se não se puder, usa-se `%APPDATA%\\Clickwork\\`, por exemplo quando o programa está instalado em `C:\\Program Files`.",
        "Os ficheiros vivem em `~/.config/clickwork` (ou `$XDG_CONFIG_HOME/clickwork`); `CLICKWORK_PORTABLE=1` mantém-nos ao lado do executável.",
    )]),
    ("appearance", Lang::Pt, &[(
        "Há nove temas, entre eles um tema Fluent que usa o Mica ou o Acrylic do Windows 11. Nesse modo a janela usa o efeito de transparência do sistema.",
        "Há nove temas, entre eles os translúcidos Fluent e Glass: a janela desenha a sua própria transparência e o compositor acrescenta o desfoque por trás, como faz o Hyprland.",
    )]),
    ("cli", Lang::Pt, &[
        ("as chamadas a `SendInput` ficam silenciadas enquanto a verificação dura.", "o envio de entrada fica silenciado enquanto a verificação dura."),
        ("pelo Agendador de Tarefas", "por um temporizador do systemd"),
    ]),
    // ---- Spanish ---------------------------------------------------------------
    ("screen-recording", Lang::Es, &[(
        "Codifica Media Foundation, la parte de Windows que sabe hablar con el codificador de la tarjeta gráfica. Si hay una NVIDIA, AMD o Intel, calcula la tarjeta; si no hay ninguna que sirva, calcula el procesador. No hay que descargar ni instalar nada aparte.",
        "Codifica `gpu-screen-recorder` o `wf-recorder`, el que esté instalado: el primero usa el codificador de la tarjeta gráfica (NVIDIA, AMD o Intel), el segundo el procesador. Sin ninguno de los dos el panel lo dice.",
    )]),
    ("first-run", Lang::Es, &[("pedirle el elemento a Windows", "pedirle el elemento al árbol de accesibilidad")]),
    ("three-ways", Lang::Es, &[
        ("«Windows, dame el botón que se llama Claim»", "«Escritorio, dame el botón que se llama Claim»"),
        ("Funciona allí donde la aplicación le cuenta a Windows qué elementos tiene su interfaz:", "Funciona allí donde la aplicación describe sus elementos en el bus de accesibilidad (AT-SPI2):"),
    ]),
    ("test-run", Lang::Es, &[("pero no envía entrada real a Windows.", "pero no envía entrada real al compositor.")]),
    ("vision", Lang::Es, &[("recortar un trozo con `Win+Mayús+S` y pulsar **Pegar**", "recortar un trozo con su herramienta de capturas (por ejemplo `grim -g \"$(slurp)\" - | wl-copy`) y pulsar **Pegar**")]),
    ("ocr", Lang::Es, &[
        ("con el OCR que trae Windows. No se distribuye ningún modelo aparte y los datos no salen del equipo.",
         "con Tesseract, abierto desde el sistema en tiempo de ejecución. No se distribuye ningún modelo aparte, los datos no salen del equipo y los idiomas son los paquetes `tesseract-data-xxx` instalados."),
        ("se usan los idiomas de la interfaz de Windows.", "se usa el idioma del escritorio más el inglés."),
        ("en un Windows en ruso leyendo", "en un sistema en ruso leyendo"),
        ("elegir `en-US` bastó", "elegir `eng` bastó"),
        ("que están instalados de verdad en Windows.", "que están instalados de verdad en el sistema."),
    ]),
    ("uia", Lang::Es, &[("Le pide a Windows un elemento de interfaz por sus propiedades,", "Le pide al bus de accesibilidad (AT-SPI2) un elemento de interfaz por sus propiedades,")]),
    ("schedule", Lang::Es, &[("**Programador de tareas de Windows**", "**temporizador de systemd o cron**")]),
    ("files", Lang::Es, &[(
        "Los archivos viven junto al ejecutable, siempre que en esa carpeta se pueda escribir. Si no, se usa `%APPDATA%\\Clickwork\\`, por ejemplo cuando el programa está instalado en `C:\\Program Files`.",
        "Los archivos viven en `~/.config/clickwork` (o `$XDG_CONFIG_HOME/clickwork`); `CLICKWORK_PORTABLE=1` los mantiene junto al ejecutable.",
    )]),
    ("appearance", Lang::Es, &[(
        "Hay nueve temas, entre ellos un tema Fluent que usa Mica o Acrylic de Windows 11. En ese modo la ventana usa el efecto de transparencia del sistema.",
        "Hay nueve temas, entre ellos los translúcidos Fluent y Glass: la ventana dibuja su propia transparencia y el compositor añade el desenfoque detrás, como hace Hyprland.",
    )]),
    ("cli", Lang::Es, &[
        ("las llamadas a `SendInput` quedan silenciadas mientras dura la comprobación.", "el envío de entrada queda silenciado mientras dura la comprobación."),
        ("por el Programador de tareas", "por un temporizador de systemd"),
    ]),
    // ---- Chinese ---------------------------------------------------------------
    ("screen-recording", Lang::Zh, &[(
        "负责编码的是 Media Foundation，也就是 Windows 里会跟显卡编码器打交道的那部分。有 NVIDIA、AMD 或 Intel 就交给显卡算；没有合适的显卡就交给处理器算。不需要另外下载或安装什么。",
        "负责编码的是 `gpu-screen-recorder` 或 `wf-recorder`，装了哪个用哪个：前者用显卡的编码器（NVIDIA、AMD 或 Intel），后者用处理器。两个都没有的话，面板会明说。",
    )]),
    ("first-run", Lang::Zh, &[("向 Windows 要一个元素", "向无障碍树要一个元素")]),
    ("three-ways", Lang::Zh, &[
        ("“Windows，把那个叫 Claim 的按钮给我。”", "“桌面，把那个叫 Claim 的按钮给我。”"),
        ("只要程序把界面元素告诉了 Windows，它就能用：", "只要程序把界面元素放到了无障碍总线（AT-SPI2）上，它就能用："),
    ]),
    ("test-run", Lang::Zh, &[("但不向 Windows 发出真实输入。", "但不向合成器发出真实输入。")]),
    ("vision", Lang::Zh, &[("用 `Win+Shift+S` 截一块然后按 **粘贴**", "用你的截图工具截一块（终端里 `grim -g \"$(slurp)\" - | wl-copy` 就行）然后按 **粘贴**")]),
    ("ocr", Lang::Zh, &[
        ("用 Windows 自带的 OCR 识别屏幕上某块区域里的文字。程序不附带单独的模型，数据也不会离开这台电脑。",
         "用 Tesseract 识别屏幕上某块区域里的文字，它在运行时从系统里打开。程序不附带单独的模型，数据也不会离开这台电脑；语言就是已安装的 `tesseract-data-xxx` 包。"),
        ("用的是 Windows 界面的语言。", "用的是桌面语言加英语。"),
        ("俄文 Windows 上识别英文游戏", "俄文系统上识别英文游戏"),
        ("选 `en-US` 之后", "选 `eng` 之后"),
        ("列表里只列出 Windows 里确实装了的识别器。", "列表里只列出系统里确实装了的语言包。"),
    ]),
    ("uia", Lang::Zh, &[("按属性向 Windows 要一个界面元素，而不是按图片去找它。", "按属性向无障碍总线（AT-SPI2）要一个界面元素，而不是按图片去找它。")]),
    ("schedule", Lang::Zh, &[("**Windows 任务计划程序**", "**systemd 定时器或 cron**")]),
    ("files", Lang::Zh, &[(
        "只要那个目录可写，文件就放在可执行文件旁边。不可写时改用 `%APPDATA%\\Clickwork\\`，比如程序装在 `C:\\Program Files` 里的时候。",
        "文件放在 `~/.config/clickwork`（或 `$XDG_CONFIG_HOME/clickwork`）；设置 `CLICKWORK_PORTABLE=1` 就放在可执行文件旁边。",
    )]),
    ("appearance", Lang::Zh, &[(
        "一共九个主题，其中包括用上 Windows 11 的 Mica 或 Acrylic 的 Fluent 主题。在那个模式下，窗口用的是系统的透明效果。",
        "一共九个主题，其中包括半透明的 Fluent 和 Glass：透明由窗口自己画，背后的模糊由合成器补上，Hyprland 就是这么做的。",
    )]),
    ("cli", Lang::Zh, &[
        ("检查期间对 `SendInput` 的调用是被静音的。", "检查期间输入的发送是被静音的。"),
        ("在任务计划程序里自动启动", "由 systemd 定时器自动启动"),
    ]),
];

/// Phrases replaced in every article of a language, longest first.
const GLOBAL: &[(Lang, &[(&str, &str)])] = &[
    (Lang::En, &[("UI Automation", "the accessibility tree")]),
    (Lang::Ru, &[
        ("UI Automation-элемент", "элемент дерева доступности"),
        ("элемент UI Automation", "элемент дерева доступности"),
        ("элементы UI Automation", "элементы дерева доступности"),
        ("запросы UI Automation", "запросы к дереву доступности"),
        ("через UI Automation", "через дерево доступности"),
        ("для UI Automation", "для дерева доступности"),
        ("в UI Automation", "в дереве доступности"),
        ("доступен UI Automation", "доступно дерево доступности"),
        ("UI Automation выше", "дерево доступности выше"),
        ("UI Automation", "дерево доступности"),
    ]),
    (Lang::Uk, &[
        ("елемент UI Automation", "елемент дерева доступності"),
        ("запити UI Automation", "запити до дерева доступності"),
        ("через UI Automation", "через дерево доступності"),
        ("для UI Automation", "для дерева доступності"),
        ("у UI Automation", "у дереві доступності"),
        ("доступний UI Automation", "доступне дерево доступності"),
        ("UI Automation вище", "дерево доступності вище"),
        ("UI Automation", "дерево доступності"),
    ]),
    (Lang::Pt, &[
        ("elemento de UI Automation", "elemento da árvore de acessibilidade"),
        ("consultas de UI Automation", "consultas à árvore de acessibilidade"),
        ("por UI Automation", "pela árvore de acessibilidade"),
        ("pelo UI Automation", "pela árvore de acessibilidade"),
        ("em UI Automation", "na árvore de acessibilidade"),
        ("há UI Automation", "há árvore de acessibilidade"),
        ("o UI Automation", "a árvore de acessibilidade"),
        ("UI Automation", "árvore de acessibilidade"),
    ]),
    (Lang::Es, &[
        ("elemento de UI Automation", "elemento del árbol de accesibilidad"),
        ("consultas de UI Automation", "consultas al árbol de accesibilidad"),
        ("por UI Automation", "por el árbol de accesibilidad"),
        ("en UI Automation", "en el árbol de accesibilidad"),
        ("hay UI Automation", "hay árbol de accesibilidad"),
        ("devolvió UI Automation", "devolvió el árbol de accesibilidad"),
        ("UI Automation", "el árbol de accesibilidad"),
    ]),
    (Lang::Zh, &[("UI Automation", "无障碍树")]),
];

/// The articles set some prepositions off with non-breaking spaces. The tables
/// here are typed with plain ones, so the body is read with plain ones too.
pub fn normalize(body: &str) -> String {
    body.replace('\u{a0}', " ")
}

/// Does `text` mention `word` as a word? `.exe` inside `exec_cmd` is not a mention.
pub fn mentions(text: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(i) = text[from..].find(word) {
        let end = from + i + word.len();
        let next = text[end..].chars().next();
        if !(word == ".exe" && next.is_some_and(|c| c.is_alphanumeric())) {
            return true;
        }
        from = end;
    }
    false
}

fn rewrite_body(id: &str, lang: Lang, body: &str) -> String {
    if let Some((_, _, whole)) = BODIES.iter().find(|(i, l, _)| *i == id && *l == lang) {
        return whole.to_string();
    }
    let mut s = normalize(body);
    for (_, _, pairs) in SPECIFIC.iter().filter(|(i, l, _)| *i == id && *l == lang) {
        for (from, to) in pairs.iter() {
            s = s.replace(from, to);
        }
    }
    if let Some((_, pairs)) = GLOBAL.iter().find(|(l, _)| *l == lang) {
        for (from, to) in pairs.iter() {
            s = s.replace(from, to);
        }
    }
    s
}

fn rewrite_title(id: &str, lang: Lang, title: &str) -> String {
    TITLES
        .iter()
        .find(|(i, l, _)| *i == id && *l == lang)
        .map(|(_, _, t)| t.to_string())
        .unwrap_or_else(|| title.to_string())
}

type Key = (&'static str, u8);
static CACHE: OnceLock<Mutex<HashMap<Key, &'static Article>>> = OnceLock::new();

/// The article as this build shows it. Rewritten once and kept.
pub fn linux_article(topic: &'static Topic, lang: Lang, original: &'static Article) -> &'static Article {
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key: Key = (topic.id, lang as u8);
    if let Some(a) = cache.lock().get(&key) {
        return a;
    }
    let title = rewrite_title(topic.id, lang, original.title);
    let body = rewrite_body(topic.id, lang, original.body);
    let made: &'static Article = if title == original.title && body == original.body {
        original
    } else {
        Box::leak(Box::new(Article {
            title: Box::leak(title.into_boxed_str()),
            body: Box::leak(body.into_boxed_str()),
        }))
    };
    cache.lock().insert(key, made);
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Lang; 6] = [Lang::En, Lang::Ru, Lang::Uk, Lang::Pt, Lang::Es, Lang::Zh];

    /// The article table the rewrite starts from, per language.
    fn raw(topic: &'static Topic, lang: Lang) -> &'static Article {
        crate::handbook::raw_article(topic, lang)
    }

    #[test]
    fn every_sentence_the_table_names_exists() {
        for (id, lang, pairs) in SPECIFIC {
            let t = crate::handbook::find(id).unwrap_or_else(|| panic!("no topic {id}"));
            let body = normalize(raw(t, *lang).body);
            for (from, _) in pairs.iter() {
                assert!(body.contains(from), "{id} / {lang:?}: not found: {from}");
            }
        }
        for (id, lang, _) in TITLES {
            assert!(crate::handbook::find(id).is_some(), "{id} {lang:?}");
        }
        for (id, lang, _) in BODIES {
            assert!(crate::handbook::find(id).is_some(), "{id} {lang:?}");
        }
    }

    #[test]
    fn no_article_mentions_the_other_platform() {
        let mut left: Vec<String> = Vec::new();
        for t in crate::handbook::all() {
            for lang in ALL {
                let a = linux_article(t, lang, raw(t, lang));
                for word in FORBIDDEN {
                    if mentions(a.title, word) || mentions(a.body, word) {
                        let at = a.body.find(word).map(|i| {
                            let before: String = a.body[..i].chars().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect();
                            let after: String = a.body[i..].chars().take(word.len() + 40).collect();
                            format!("{before}{after}")
                        }).unwrap_or_else(|| "(title)".into());
                        left.push(format!("{} / {lang:?}: {word:?} in …{}…", t.id, at.replace('\n', " ")));
                    }
                }
            }
        }
        assert!(left.is_empty(), "still there:\n{}", left.join("\n"));
    }

    #[test]
    fn no_interface_string_mentions_the_other_platform() {
        for i in 0..6 {
            let table = crate::tables()[i];
            for (field, text) in table.to_map() {
                for word in FORBIDDEN {
                    assert!(!mentions(text, word), "table {i}, {field}: {text}");
                }
            }
        }
        for name in crate::THEME_NAMES {
            for word in FORBIDDEN {
                assert!(!name.contains(word), "theme {name}");
            }
        }
    }

    #[test]
    fn rewritten_articles_keep_the_markup_rules() {
        // Same rule the Windows handbook test enforces: no stray single stars.
        for t in crate::handbook::all() {
            for lang in ALL {
                let a = linux_article(t, lang, raw(t, lang));
                // Bold is `**`, italics a single pair; what must never happen is an
                // odd number of single stars.
                for line in a.body.lines() {
                    let singles = line.matches('*').count() - line.matches("**").count() * 2;
                    assert!(singles % 2 == 0, "{} / {lang:?}: odd stars in: {line}", t.id);
                }
            }
        }
    }
}
