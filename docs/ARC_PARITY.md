# Паритет с Arc

Сравнение с Arc для macOS версии 1.169.0 (`/Applications/Arc.app`). Наш код:
`crates/core`, `crates/browser`, `crates/engine` на дату документа.

**Откуда данные.** Arc не запускали. Всё взято из файлов бандла:
главное меню в `Base.lproj/MainMenu.nib` (пункты, клавиши, синонимы
для Command Bar), AppleScript-словарь `Arc.sdef`, строки интерфейса
и комментарии переводчиков из бинарника (`Contents/MacOS/Arc`, сгруппированы
по исходным модулям: `PreferencesFeature`, `ContextMenus`, `TypeTravel`,
`CommandBar`, `SpaceStyleEditor`, `Toasts`, `LittleBrowser`, `PictureInPicture`
и др.), ресурсные бандлы `ARC_*.bundle`. Пометка **(?)** значит, что пункт
выведен косвенно. Например, клавиша не записана в nib и назначается в коде,
или строка есть, но её точное место в интерфейсе неизвестно.

Easels не входят в сравнение по решению пользователя. Notes (документы Arc)
упомянуты один раз.

Статусы: **сделано**, **частично**, **нет**.

---

## 1. Главное меню и горячие клавиши

Клавиши взяты из nib. У пунктов меню есть `arcKeyBindingID`, поэтому
пользователь может их переназначить (см. раздел «Настройки → Shortcuts»).
Пункты с пометкой «скрыт» есть в меню, но не видны. Обычно это
альтернативные клавиши.

### Arc (меню приложения)

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| About Arc | — | нет | Окно «О программе» с лицензиями (в Arc это `Credits.html`) |
| Preferences… | ⌘, | нет | Окна настроек нет совсем |
| Share Arc (реферальная ссылка) | — | нет | Вне паритета (сервис Arc) |
| Set as Default Browser | — | нет | Регистрация браузером по умолчанию, промпты |
| Import from Another Browser… | — | нет | См. «Онбординг/импорт» |
| Check for Updates… | — | нет | Автообновление |
| Privacy Policy | — | нет | — |
| Hide Arc / Hide Others / Show All | ⌘H / ⌥⌘H / — | нет | Стандартные пункты меню не заведены (у нас в меню только Quit) |
| Sign Out | — | нет | Аккаунта нет |
| Quit | ⌘Q | сделано | Подтверждения «Warn before quitting» нет |

### File

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| New Tab… (открывает Command Bar) | ⌘T | сделано | — |
| New Window | ⌘N | нет | Окно у нас одно |
| Blank Window | ⌃⌘N | нет | — |
| New Incognito Window | ⇧⌘N | нет | Инкогнито-окно с отдельным сайдбаром («Incognito Tabs») |
| New Little Arc Window | ⌥⌘N | нет | См. «Little Arc» |
| Restore Last Closed Tab | ⇧⌘T | сделано | Возвращает последнюю вкладку из архива в её Space |
| Open Command Bar (на текущем URL) | ⌘L | сделано | — |
| New Boost / Edit Boost / Boost Gallery | скрыт | нет | См. «Boosts» |
| New Profile | — | нет | Профилей нет |
| New Note | ⌃⇧N | нет | Заметки (Arc Notes) |
| New Note (in Split) | ⌃⌥N (скрыт) | нет | — |
| Close Window | ⇧⌘W | нет | — |
| Archive Tab | ⌘W | сделано | Today уходит в архив, закреплённая закрывается, остаётся в Pinned и возвращается к закреплённому адресу |
| Share… | — | нет | Системное меню «Поделиться» |
| Capture… | ⇧⌘2 | нет | Снимок области страницы |
| Capture Full Page / Capture in Portrait Mode | — | нет | — |
| Save Page As | ⇧⌘S | нет | — |
| Print | ⌘P | нет | — |

### Edit

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| Undo / Redo (в том числе для действий сайдбара: «Undo moving a space», «Close Peek» и т.п.) | ⌘Z / ⇧⌘Z | нет | Undo-менеджера для сайдбара нет. Undo в поле ввода Command Bar тоже не проверялось (?) |
| Cut / Copy / Paste / Select All | ⌘X/⌘C/⌘V/⌘A | частично | Работают в нашем `TextInput` и на странице через CEF (?). Пунктов меню нет |
| Copy URL (чистая ссылка без трекеров) | ⇧⌘C | нет | Ещё нужен тост «Copied a clean link without trackers» |
| Copy URL as Markdown | ⌥⇧⌘C | нет | — |
| Copy URL as Quote (ссылка на выделенный текст) | ⌃⇧⌘C | нет | — |
| Paste and Match Style | ⇧⌘V | нет | — |
| Find… / Find and Replace / Find Next / Find Previous / Jump to Selection | ⌘F / ⌥⌘F / ⌘G / ⇧⌘G / ⌘J | нет | Поиска по странице нет |
| Spelling, Substitutions, Transformations, Speech, Format (Bold/Italic/Underline) | ⌘: ⌘; ⌘B ⌘I ⌘U | нет | Стандартные подменю macOS |

### View

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| Appearance: сайты используют Automatic / Light / Dark | — | нет | — |
| Show/Hide Sidebar | ⌘S | сделано | — |
| Show/Hide Toolbar (режим верхней панели) | ⇧⌘D (скрыт) | нет | Панели с URL вверху нет |
| Expand/Collapse Pinned Section | скрыт | нет | — |
| Reveal Current Tab | ⌃Space (скрыт) | нет | — |
| Stop Loading | ⌘. | нет | В движке есть reload, а stop не выведен |
| Refresh the Page / Force Refresh | ⌘R / ⇧⌘R | частично | ⌘R есть, ⇧⌘R (без кэша) нет |
| Clear Cookies and Refresh / Clear Cache and Refresh | — | нет | — |
| Add Split View | ⌃⇧= | нет | См. «Split View» |
| Close this Split Pane | ⌃⇧- | нет | — |
| Separate Page from Split View / Expand Current Split | — | нет | — |
| Zoom to Actual Size / Zoom In / Zoom Out | ⌘0 / ⌘+ (⌘=) / ⌘- | нет | — |
| Cast | — | нет | — |
| Developer: View Source, Developer Tools, Inspect Elements, JavaScript Console, Network Inspector | ⌥⌘U, ⌥⌘I, ⌥⌘C, ⌥⌘J, — | нет | DevTools через CEF не подключены |
| Developer: Allow JavaScript from Apple Events | — | нет | — |
| Toggle Developer Mode (для сайта) | ⌃D | нет | — |
| Enter/Exit Full Screen | ⌃⌘F | нет | Есть ли полноэкранный режим через системную кнопку, не проверялось (?) |
| Exit (выход из контекста: Peek, полный экран) | ⌘Esc | нет | — |

### Spaces

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| New Space… | — | нет | Создать Space из интерфейса нельзя, есть только демо-данные |
| Edit Theme… | — | нет | См. «Темы Spaces» |
| Rename Space | — | нет | — |
| Change Profile | — | нет | — |
| Next Space / Previous Space | ⌥⌘→ / ⌥⌘← | сделано | — |
| Перейти к Space N («Jumps over to your %@ Space») | ⌃1…⌃9 (?) | сделано | Клавиши по умолчанию в nib не записаны |
| Manage Spaces… (Space Manager в Library) | — | нет | — |

### Tabs

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| New Tab… | ⌘T | сделано | — |
| New ChatGPT Message… | ⌥⌘G (скрыт) | нет | Функция Arc Max |
| Join Meeting (ссылка на ближайшую встречу) | ⌃⌘J (скрыт) | нет | — |
| Peek Email… (код из письма) | ⇧⌘E (скрыт) | нет | — |
| Pin/Unpin Tab | ⌘D | сделано | — |
| Open Little Arc or Peek in… (выбор Space) | ⌘O | нет | — |
| New Folder… | ⇧⌘N по nib (?) | нет | В nib стоит та же клавиша, что у Incognito. Реальная раскладка задаётся в коде |
| Duplicate / Rename Current Item / Collapse All Folders / Expand All Folders | скрыт | нет | — |
| Next Tab / Previous Tab | ⌥⌘↓ / ⌥⌘↑ | сделано | — |
| Tab Switcher вперёд/назад (оверлей «Recent Items», MRU, как ⌘Tab) | ⌃Tab / ⌃⇧Tab | частично | У нас ⌃Tab листает вкладки по порядку сайдбара. Оверлея и порядка по недавности (MRU) нет |
| Cycle Tab Forward/Backward | ⌃⌘→ / ⌃⌘← (скрыт) | нет | — |
| Go to Sidebar Item 1–8 / last item (с Favorites или без) | ⌘1…⌘9 (?) | частично | Pinned, затем Today. Нет Favorites и опции «Include Favorites in ordering» |
| Switch to Split View 1–4 / next / previous | (?) | нет | — |
| Reveal Tab in Sidebar | — | нет | — |
| Clear Unpinned Tabs (Today) | ⇧⌘K | сделано | — |
| Reset all tabs in this Space | — | нет | — |
| Reset Tab to Original URL (закреплённой) | скрыт | нет | — |

### Archive (меню называется так)

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| Go Back / Go Forward | ⌘[ / ⌘] (и ⌘← / ⌘→ скрыт) | частично | ⌘[ и ⌘] есть, ⌘← и ⌘→ нет. Нет меню истории по долгому нажатию на «Назад» |
| View History | ⌘Y | нет | В Arc открывает историю в новой вкладке Today |
| View Archive | — | нет | — |
| Clear Archive (с подтверждением «This action is permanent.») | — | нет | — |

### Extensions / Window / Help

| Функция | Клавиша Arc | Статус | Чего не хватает |
|---|---|---|---|
| Add Extension… / Manage Extensions… | — | нет | Расширений Chrome нет |
| Minimize / Zoom / Bring All to Front | ⌘M | нет | — |
| View Library | ⇧⌘L | нет | См. «Library» |
| View Downloads | ⇧⌘J | нет | — |
| View Media / View Boosts | — | нет | — |
| Help: Getting Started, Essential Keyboard Shortcuts, Release Notes, Contact the Team, Help Center, Restore Data, Export Arc Notes | — | нет | Важны для нас «Essential Keyboard Shortcuts» (окно-шпаргалка) и «Restore Data» |
| Troubleshooting: Record Trace, Task Manager, Reveal Arc Data, Copy Arc Info | — | нет | — |

---

## 2. Сайдбар

| Функция | Клавиша/жест | Статус | Чего не хватает |
|---|---|---|---|
| Вертикальный сайдбар со вкладками | — | сделано | Ширину нельзя менять (?) |
| Кнопки окна (светофор) внутри сайдбара | — | сделано | — |
| Адресное поле вверху сайдбара (клик открывает Command Bar с URL) | — | сделано | Нет кнопок назад/вперёд/обновить и кнопки Site Control Center |
| Скрытие сайдбара и выезд при наведении на левый край | ⌘S | сделано | — |
| Режим «Show as Toolbar» (контекстное меню: Show as Toolbar / Hide Toolbar) | ⇧⌘D | нет | — |
| **Favorites** (Top Apps): сетка иконок над Spaces, общая для всех Spaces профиля | — | нет | Нужны перетаскивание («Drag to add Favorites»), бейджи непрочитанного (Gmail, GitHub, X, WhatsApp), подписи календаря («Show Time to Next Meeting») |
| Hover-превью для Favorites, Pinned и папок (Gmail, Google/Outlook Calendar, Linear, Notion, Figma, Confluence) | наведение | нет | Настраивается в «Previews Settings…» |
| **Pinned** секция Space | ⌘D | частично | Нет «Edit Pinned Page» (свой URL), «Replace Pinned URL with Current», «Reset Tab to Original URL». Закрытие должно оставлять вкладку в Pinned («Close and Keep Pinned»), а у нас удаляет. Нет сворачивания секции |
| **Today** секция (незакреплённые) и кнопка «+ New Tab» | — | сделано | — |
| Кнопка «Clear» над Today (архивирует незакреплённые) | ⇧⌘K | частично | Работает только клавиша, кнопки нет |
| «Tidy Tabs»: группировка Today по темам с помощью ИИ | — | нет | Функция Arc Max, можно отложить |
| Папки: вложенные, свернуть/развернуть, сделать Space из папки, папка из выделенных вкладок | ⇧⌘N (?) | нет | — |
| Live Folders (GitHub Pull Requests: Created by Me, Review Requests…) | — | нет | — |
| Перетаскивание вкладок (между секциями, в папки, в Favorites, в другой Space) | drag | нет | — |
| Тактильный отклик при перестановке вкладок | — | нет | Опция в Advanced |
| Мультивыделение вкладок (⌘/⇧-клик) | — | нет | — |
| Переименование вкладки двойным кликом («Double-click to rename») | двойной клик | нет | Нужен ещё сброс «Reset Name and Icon» |
| Своя иконка вкладки («Customize Tab») | — | нет | — |
| Кнопка закрытия при наведении (× для Today, − для Pinned) | наведение | сделано | — |
| Индикатор звука, Mute/Unmute в контекстном меню | — | нет | — |
| Индикатор камеры/микрофона на вкладке | — | нет | — |
| Элемент Split View в сайдбаре (группа вкладок) | — | нет | — |
| Заголовок Space (имя), Show/Hide Space Header | — | частично | Имя показывается, скрыть его нельзя |
| Док Spaces внизу (иконки Spaces) | клик | частично | У нас цветные точки, а в Arc иконки или эмодзи Spaces |
| Кнопка «+» в доке: New Tab, New Space, New Folder, New Split (справа/снизу), New Note, New Boost | — | нет | — |
| Анимация «пружинящего» клика по ячейкам | — | нет | — |
| Сайдбар инкогнито-окна («Incognito Tabs») | — | нет | — |
| Баннеры внизу сайдбара: обновление, «We cleaned up!» после автоархивации, ускорение (несколько блокировщиков рекламы) | — | нет | — |
| Медиаконтролы в сайдбаре | — | нет | См. «Медиа и PiP» |

## 3. Spaces

| Функция | Клавиша/жест | Статус | Чего не хватает |
|---|---|---|---|
| Несколько Spaces с отдельными Pinned/Today и своей активной вкладкой | — | сделано | — |
| Свайп двумя пальцами по сайдбару: страницы Spaces идут за пальцами, пружинят на краях | жест | сделано | — |
| Переключение Space с клавиатуры (с анимацией) | ⌥⌘→/← | сделано | — |
| Создание Space (диалог «Create a Space»: имя, иконка, тема, профиль) | — | нет | — |
| Переименование Space | — | нет | — |
| Иконка Space (эмодзи или символ, «Change Space Icon…», «Remove icon») | — | нет | — |
| Удаление Space (с Undo) | — | нет | — |
| Изменение порядка Spaces («Move Space») | drag | нет | — |
| Сделать из Space папку («Export “%@” into Folder») | — | нет | — |
| Live Folders для Space | — | нет | — |
| Space Manager (Library → Spaces) | — | нет | — |
| Привязка профиля к Space («Set Profile», «Edit Space profile») | — | нет | — |

## 4. Темы и цвета Spaces (Space Style Editor)

| Функция | Статус | Чего не хватает |
|---|---|---|
| Цвет Space окрашивает сайдбар и окно, при свайпе цвет плавно переходит | частично | У нас только оттенок (hue) и плавный переход при свайпе. Градиентов и зерна нет |
| Редактор темы («Edit Theme Color…», «Tap to pick a color for this space») | нет | — |
| Палитра с несколькими цветами («Add color» / «Remove color»), градиент из 2–3 цветов (?) | нет | — |
| Готовые наборы цветов, листаются страницами: Brand, Drab, Greyscale, Pastel | нет | — |
| Текстура шума («Cycle noise texture») | нет | — |
| Ползунок «Intensity» | нет | — |
| Режим: Automatic / Light / Dark Appearance | нет | — |
| Undo «Update Theme» | нет | — |
| Сайты могут читать тему («Allow websites to get your theme data», CSS-переменные `--arc-palette-*`) | нет | Опция в Advanced |

## 5. Command Bar (⌘T / ⌘L)

| Функция | Клавиша | Статус | Чего не хватает |
|---|---|---|---|
| Всплывающее окно поверх страницы | ⌘T | сделано | — |
| Ввод URL или поиск («Search or Enter URL…») | — | сделано | Поисковик зашит (Google). В Arc он выбирается в настройках профиля |
| Редактирование текущего URL | ⌘L | сделано | — |
| Навигация ↑/↓, ⌃N/⌃P, Enter, Esc, клик вне окна закрывает | — | сделано | — |
| Переключение на открытую вкладку | — | частично | У нас только вкладки текущего Space. В Arc вкладки всех Spaces с подписью «… and switch there» и в Split View |
| Подсказки поисковика (Chromium: Search Suggestion, Navigate Suggest) | — | нет | Нужен ещё экран о приватности подсказок |
| История и закладки в подсказках (Chromium: History / Bookmark / Search History) | — | нет | — |
| Калькулятор (Search Calculator Answer) | — | нет | — |
| Популярные домены (Popular Domains, ARC_TopDomains) | — | нет | — |
| Site Search: Tab или Space после имени сайта ищет по сайту, есть меню управления | Tab / Space | нет | — |
| Удаление подсказки («Delete Suggestion») | — | нет | — |
| **Действия (Type Travel)**: каждый пункт меню доступен по названию и синонимам (`arcTypeTravelTriggers` в nib) | — | нет | Это центральная фича Command Bar Arc |
| Действия Space: «Focus on <space>», «Switch to …» | — | нет | — |
| Перенос вкладки: «Move to Pinned in <space>», «Move to Today in <space>», «Move to Folder» | — | нет | — |
| Favorites: «Favorite Tab», «Unfavorite Tab», «Pin to Top Apps» | — | нет | — |
| Split: Add Top/Bottom/Left/Right Split, Convert to Vertical/Horizontal | — | нет | — |
| Папки: Create New Folder, Collapse/Expand All Folders | — | нет | — |
| Вкладки: Duplicate Current Tab, Rename Current Tab…, Reset Tab, Replace Pin with Current Page, Mute/Unmute All Tabs | — | нет | — |
| Прочее: Toggle Sidebar, Manage Passwords, Search Settings, Air Traffic Control, Edit Keyboard Shortcuts, Check for Updates, Undo/Redo <действие> | — | нет | — |
| Запуск расширения по имени («Extension …») | — | нет | — |
| Join next video call / Zoom | — | нет | — |
| Подсказки при пустом поле (Air Traffic Control, Tips for Organizing, Help Center) | — | нет | — |
| ChatGPT в Command Bar, «Browse for Me» (Arc Max) | — | нет | ИИ-функции, приоритет ниже |

## 6. Контекстные меню

Строки взяты из модуля `ContextMenus/SidebarMenu+*`. Короткие названия
пунктов (до 15 байт: Copy Link, Rename…, Duplicate и т.п.) хранятся
в коде отдельно, поэтому часть из них восстановлена по комментариям (?).

| Меню / пункт | Статус | Чего не хватает |
|---|---|---|
| **Вкладка**: Copy Link, Copy link as Markdown, Share, Duplicate, Rename (?), Edit Pinned Page → Replace Pinned URL with Current, Add/Open in Split View, Move to… (Spaces, профиль), New Folder with…, Mute/Unmute, Archive Tab / Close Tab / Remove Pin / Remove Favorite, Archive Other Tabs / Tabs Below / Tabs Above, Customize Page… (Boost) | нет | Контекстных меню у нас нет совсем |
| **Мультивыделение**: Archive Items, Copy links (as Markdown), New Folder with N items, Move to… | нет | — |
| **Папка**: Share, Duplicate, Archive Folder / Delete, New Nested Folder…, Turn into Space, Copy All Links (as Markdown) | нет | — |
| **Live Folder**: Pull Requests, Repositories, Single Sign-On, Delete Live Folder | нет | — |
| **Split View в сайдбаре**: Share Split View…, Separate All Tabs, Add Split View, Convert to Vertical/Horizontal, Tab Options → Reset Name and Icon, Duplicate, Archive…, Separate from Split View | нет | — |
| **Панель Split View** (меню управления): Separate Tab from Split, Share Split View…, Convert orientation, Move Left/Right/Up/Down, Add Left/Right/Top/Bottom Split | нет | — |
| **Space**: Change Space Icon…, Rename, Edit Theme Color…, Set Profile, New Folder, Live Folders, Share, Show/Hide Space Header, Manage Spaces…, Delete Space, New Profile, Export into Folder | нет | — |
| **Favorite с календарём** (Live Calendar): Show Hover Preview, Show Time to Next Meeting, Show Next Meeting Link, напоминание за 2/5/10 минут | нет | — |
| **Загрузка**: Open, Copy, Show in Finder, Cancel Download, Hide from Arc, Move to Trash | нет | — |
| **Заголовок группы вкладок**: Remove header | нет | — |
| **Страница** (меню Chromium плюс пункты Arc) (?) | нет | Через CEF контекстное меню страницы не настроено (?) |

## 7. Архив и автоархивация

| Функция | Статус | Чего не хватает |
|---|---|---|
| Автоархивация Today-вкладок после простоя, по умолчанию 12 часов | сделано | Таймер раз в минуту в `window.rs`, логика в `core::archive_idle` |
| Интервалы: Never, 1 hour (?), 6 hours, 12 hours, 24 hours, 7 days, 30 days | частично | Модель поддерживает любой интервал и «никогда», а выбора в интерфейсе нет. Варианты 1h и 6h видны только как строки порогов, есть ли они в меню, неясно (?) |
| Настройка на профиль и вопрос «применить ко всем профилям / только к этому» | нет | — |
| Отдельный интервал «Archive Little Arcs after:» | нет | — |
| ⌘W и × отправляют вкладку в архив | сделано | — |
| Страница Archive в Library: «Today/Yesterday», фильтры по Space, «How was the tab closed?» (Auto-Closed/Manually Closed), Little Arc | нет | — |
| Restore («Tab restored to …. Click to view.»), Delete, Clear Archive | нет | — |
| Тост «Auto archived %d tabs» и баннер «We cleaned up!» | нет | — |

## 8. Split View

| Функция | Клавиша | Статус | Чего не хватает |
|---|---|---|---|
| Добавить панель (до 4 панелей, тост «Cannot Add New Pane») | ⌃⇧= | нет | — |
| Закрыть панель | ⌃⇧- | нет | — |
| Создать перетаскиванием вкладки в центр окна | drag | нет | — |
| Горизонтальная/вертикальная раскладка, Move Left/Right/Up/Down | — | нет | — |
| Separate Page / Separate All Tabs / Expand Current Split | — | нет | — |
| Фокус на панели N / следующей / предыдущей | (?) | нет | — |
| Название Split View («Split view title text field placeholder») | — | нет | — |
| Share Split View (ссылка) | — | нет | Сервис Arc |

## 9. Little Arc

| Функция | Клавиша | Статус | Чего не хватает |
|---|---|---|---|
| Маленькое окно для быстрых запросов | ⌥⌘N | нет | — |
| Глобальная горячая клавиша «Open Little Arc when I press … in any app» | настраивается | нет | — |
| Ссылки из других приложений открываются в Little Arc | — | нет | — |
| ⌥⌘+клик по ссылке открывает её в Little Arc | — | нет | — |
| Панель: Open in (Space, «More Spaces», «Search Spaces…»), Change profile, Extensions, Copy URL, загрузки | ⌘O | нет | — |
| Show/Close all Little Arc windows | — | нет | — |
| Подсказка для новичков «Links Open in Little Arc» | — | нет | — |

## 10. Peek

| Функция | Статус | Чего не хватает |
|---|---|---|
| Ссылки на другие сайты из Pinned/Favorites открываются в Peek (опция) | нет | — |
| Shift+клик открывает ссылку в Peek (опция) | нет | — |
| Закрытие Peek (⌘Esc / Esc), Undo «Close Peek» | нет | — |
| Перенос Peek во вкладку или в Space («Open Little Arc or Peek in…» ⌘O) | нет | — |
| Свайп для закрытия модального окна (`modal-v2-native-gestures`) (?) | нет | — |

## 11. Air Traffic Control (маршрутизация ссылок)

| Функция | Статус | Чего не хватает |
|---|---|---|
| Правила «New Route»: URL или домен «Is equal to» … → Space / Most Recent Space / Little Arc | нет | — |
| Доступ из Links → «Air Traffic Control…» и из Command Bar | нет | — |
| Варианты открытия ссылки: новая вкладка (с фокусом и без), Split View, Peek, новое окно, Little Arc | частично | Всплывающие окна (`window.open`, target=_blank) у нас открываются новой вкладкой. Остальных режимов нет |

## 12. Профили

| Функция | Статус | Чего не хватает |
|---|---|---|
| Профиль: отдельные история, cookies, пароли, расширения | нет | Движок работает с одним кэшем `Default` |
| Создание профиля: имя и выбор Spaces, которые его используют | нет | — |
| Смена профиля Space с предупреждениями («Switch to your … Profile?») | нет | — |
| Удаление профиля вводом имени, «Delete unused Profiles». Профиль по умолчанию удалить нельзя | нет | — |
| Настройки на профиль: поисковик, подсказки поиска, автоархивация, папка загрузок, приложение для документов, очистка данных | нет | — |
| Профили не синхронизируются между компьютерами (так и в Arc) | — | Справка |

## 13. Настройки (Preferences, ⌘,)

Вкладки: General, Profiles, Links, Shortcuts, Max, Advanced, Account
(синхронизация), Icon. Есть ещё служебная Staff. Окна настроек у нас нет,
поэтому статус всех пунктов ниже **нет**.

| Вкладка → опция | Статус |
|---|---|
| General → Arc не браузер по умолчанию (кнопка «Set as default») | нет |
| General → «Profile Settings…» (поисковик, автоархив, загрузки) | нет |
| General → «Archive tabs after» | нет |
| General → Встроенный блокировщик: Block Ads, Block Trackers, Block Cookie Banners, «Ad Block Settings…» (Advanced Ad Block Settings), совет оставить один блокировщик | нет |
| General → «Previews Settings…» (Show Arc Previews: Gmail, Calendar, Outlook, Linear/Notion/Figma) | нет |
| General → Warn before quitting | нет |
| General → Automatically update my Arc | нет |
| General → Export Data (Spaces и вкладки в Markdown, «Arc Data Export.md») | нет |
| Profiles → список профилей, создание и удаление, Default Search Engine, Include search engine suggestions, Search Settings…, автоархив, Download Location, Default Document App, Privacy and Security → Clear Browsing Data | нет |
| Links → Air Traffic Control…, Peek (2 опции), Little Arc (горячая клавиша, ссылки из других приложений, ⌥⌘-клик, Archive Little Arcs after) | нет |
| Shortcuts → поиск («Type a feature name or shortcut»), Default / Customized Shortcuts, запись своей клавиши, конфликты («is already in use by …», Remove Conflicting Shortcut), приоритет на клавишу (Arc wins / Website wins / первое нажатие сайту, второе Arc), Reset Shortcut / Reset All, Extension Shortcuts…, «Include Favorites in ordering», «Learn the Essential Shortcuts» | нет |
| Max → Ask on Page (⌘F), Tidy Tab Titles, Tidy Downloads, ChatGPT in the Command Bar, 5-Second Previews, Automatically enable future Max features | нет (ИИ, приоритет ниже) |
| Advanced → More Settings… (arc://settings), Reset Translated-site Prompts, Enable Boosts, Allow websites to get your theme data, Picture in Picture при уходе с вкладки с видео, перетаскивание окна за верх страницы, Shared Quotes, Show full URL when Toolbar is enabled, восстановление окон при запуске, Haptic feedback при перестановке, Play Arc sound effects | нет |
| Account → Arc Sync (E2E), Recovery Card и фраза восстановления, смена имени, email и пароля, Delete Account, удаление данных синхронизации, устаревшая iCloud Sync | нет |
| Icon → альтернативные иконки приложения (открываются за рефералов) | нет (не копируем ассеты) |

## 14. Library (⇧⌘L)

| Раздел | Статус | Чего не хватает |
|---|---|---|
| Spaces (Space Manager: порядок Spaces, папок, вкладок) | нет | — |
| Archive (см. раздел 7) | нет | — |
| Downloads (⇧⌘J): фильтр, фильтр по профилям, анимация «загрузка улетает в Library», «File deleted or moved» | нет | — |
| Media: Captures и файлы из Desktop/Documents/Downloads, Open in Preview, Show in Finder, Hide from Arc, Copy | нет | — |
| Boosts: список, вкл/выкл, переименование, удаление | нет | — |
| Easels & Notes | — | Вне сравнения (Easels). Notes имеют низкий приоритет |

## 15. Boosts

| Функция | Статус |
|---|---|
| Редактор: цвет (color wheel, Brightness, Original Saturation, Invert lightness, Advanced color controls), шрифт, размер и регистр текста | нет |
| Zap: выбор элемента для скрытия (Pick Selector, «Zap all related elements») | нет |
| Свой CSS/JS («Refresh to Run»), Boosts с JS нельзя шарить | нет |
| Включение и выключение на сайт, «Customize Page…» в контекстном меню, раздел Boosts в Site Control Center | нет |
| Шаринг и установка чужого Boost («Add this Boost to Arc?»), Boost Gallery | нет (сервис Arc) |

## 16. Страница, верхняя панель и Site Control Center

| Функция | Статус | Чего не хватает |
|---|---|---|
| Страница во встроенной «карточке» со скруглёнными углами | сделано | — |
| Индикатор загрузки (светящийся прогресс, `GlowProgressLoadingIndicator`) | нет | Событие загрузки из движка приходит, но не отображается |
| Site Control Center: значок безопасности, Copy URL, Capture / Full Page / Portrait, Share, Boosts, расширения (pin/unpin, Add Extension), Clear Cache, Clear Cookies, All Site Settings… | нет | — |
| Статус-строка с URL при наведении на ссылку (`StatusTextView`) | нет | — |
| Find in Page («No results found»), Find and Replace | нет | — |
| Перевод страницы: «Translate this page to %@?», Use Original, Choose Another Language, Never Translate This Site | нет | — |
| Страница сбоя вкладки (Sad Tab) | нет | — |
| Свайп назад/вперёд по странице (overscroll) | нет (?) | Не проверялось, работает ли жест в CEF |
| Перетаскивание окна за верх страницы (опция) | нет | — |
| Захват указателя: тост «Your cursor is hidden. Press 'esc'…» | нет | — |
| Полноэкранный режим страницы: тост «To exit full screen, press …» | нет | — |
| Режим разработчика для сайта, DevTools | нет | — |
| Расширения Chrome (иконки, «Open pinned extensions picker») | нет | — |
| Встроенный блокировщик рекламы и трекеров | нет | — |
| Тихое создание вкладки при скрытом сайдбаре («New Tab Created in %@») | нет | — |

## 17. Медиа и Picture-in-Picture

| Функция | Статус | Чего не хватает |
|---|---|---|
| Мини-плеер в сайдбаре: Back to Tab, Open Picture in Picture, Previous/Next Track, Adjust Tab Volume, Toggle Microphone/Camera, Close Mini Player | нет | — |
| Автоматический PiP при уходе с вкладки с видео (опция) | нет | — |
| Окно PiP: Back to Tab, ±15 секунд, перемотка перетаскиванием вверх, Keep Window on Top, свернуть или закрыть, запоминание позиции | нет | — |
| PiP для Google Meet (с чатом) и Document PiP | нет | — |
| Mute/Unmute вкладки, Mute All / Unmute All | нет | — |

## 18. Уведомления и тосты

Ниже основные тосты из модуля `Toasts`. У нас нет механизма тостов вообще.

| Тост | Статус |
|---|---|
| Undo для действий («“%@”. ⌘Z to Undo.») | нет |
| Copied Current URL / clean link without trackers / Copied item links / as quote | нет |
| Pinned Tab to “%@” / Un-Pinned Tab from “%@” | нет |
| Auto archived %d tabs; Tab/Folder restored to … | нет |
| New Tab Created in %@ (при скрытом сайдбаре); New Space Created | нет |
| Cannot Add New Pane (Split View) | нет |
| Импорт: Now importing… / Success / partial / failure | нет |
| Перевод: Translated to %@ / back to %@ / ошибка | нет |
| Обновления: update pending / failed / in progress | нет |
| Battery Saver Mode Activated/Deactivated | нет |
| Ошибки синхронизации (password change, outdated device и т.п.) | нет |
| Подсказки о клавишах («Copy links faster by pressing…», «… to easily show and hide your Sidebar!», Site Search) | нет |
| Screenshot Copied to Clipboard, Capture in Progress… | нет |

## 19. Онбординг и импорт

| Функция | Статус | Чего не хватает |
|---|---|---|
| Вход и регистрация в аккаунте Arc | нет | Вне паритета (сервис Arc). Для нас аналог — локальный профиль |
| Импорт из Chrome-подобных браузеров, Safari, Firefox: история, cookies, пароли (Passwords.csv), автозаполнение, закладки с выбором папок | нет | — |
| Выбор профиля для импорта («My Existing Arc Profile» / «Create a New Profile») | нет | — |
| Шаги: тема («Add some flair with a theme.»), Favorites («Choose the apps you use most.»), блокировщик рекламы, Arc Max, браузер по умолчанию, карточка участника | нет | — |
| Экран «Let’s settle in! Here are the basics.» (Pin tabs, Split View, Spaces, Open your first tab, с видео) | нет | — |
| Окно «Essential Keyboard Shortcuts» (⌘T, ⌃Tab, ⇧⌘C, ⌃⇧=, ⌥⌘N) | нет | — |
| Пустое состояние: «Open your first tab. Click New Tab or press ⌘T…» | частично | У нас заглушка «No tab open / Press ⌘T to open a new tab» |
| Промпты браузера по умолчанию («Try Arc as default for one week» и т.п.) | нет | — |

## 20. Синхронизация

| Функция | Статус | Чего не хватает |
|---|---|---|
| Сохранение сессии между запусками (Spaces, вкладки, архив) | сделано | `session.json` в папке профиля, демо-данные только при первом запуске. Сохраняются адреса, но не история «назад/вперёд» |
| «When opening Arc, restore windows from previous session» | нет | — |
| Arc Sync: синхронизация сайдбара (Spaces, Pinned, папки) между устройствами со сквозным шифрованием и фразой восстановления | нет | — |
| Синхронизируемые настройки (`SyncedPreferences`) | нет | — |
| Cookies сохраняются между запусками | сделано | `persist_session_cookies` в движке |

## 21. Жесты и «ощущение»

| Функция | Статус | Чего не хватает |
|---|---|---|
| Свайп по сайдбару: Spaces идут за пальцами, сопротивление на краях, щелчок по скорости, возврат при коротком свайпе | сделано | — |
| Анимированное переключение Space с клавиатуры | сделано | — |
| Выезд скрытого сайдбара от края с задержкой скрытия | сделано | — |
| Свайп назад/вперёд по странице | нет (?) | — |
| Перетаскивание вкладок с анимацией, перетаскивание в центр для Split | нет | — |
| Тактильный отклик и звуки Arc (`ARC_SoundEffects`) | нет | — |
| Пружинящий клик по ячейкам сайдбара | нет | — |

## 22. AppleScript (`Arc.sdef`)

| Функция | Статус | Чего не хватает |
|---|---|---|
| Классы: window (active tab, active space, incognito, mode), tab (id, title, URL, loading, location: topApp/pinned/unpinned), space (id, title) | нет | macOS-специфично. Можно заменить кроссплатформенным API автоматизации (?) |
| Команды: make, close, select, go back, go forward, reload, stop, execute (JavaScript), focus (space) | нет | — |

## 23. Прочее

| Функция | Статус | Чего не хватает |
|---|---|---|
| Capture (снимок области, ⇧⌘2), сохранение в Library | нет | — |
| Тихие загрузки и трей загрузок | нет | — |
| Режим экономии батареи (Battery Saver) | нет | — |
| Task Manager, запись трассировки | нет | — |
| Shared Quotes (ссылка на выделенный текст) | нет | — |
| Arc Max (ИИ: Ask on Page, Tidy Titles и Downloads, 5-Second Previews, ChatGPT) | нет | Приоритет ниже основного паритета |

---

## Итог

Подсчёт по строкам таблиц выше. Пункты-справки без статуса не считались.

- **сделано**: 33
- **частично**: 14
- **нет**: 243

### Главные пробелы (по важности для паритета)

1. **Command Bar как центр управления.** Нужны действия по названию
   (Type Travel) из всех пунктов меню, вкладки всех Spaces, история,
   подсказки поисковика, Site Search по Tab. Сейчас есть только URL,
   поиск и вкладки текущего Space.
2. **Архив.** Нужны страница Archive с восстановлением, выбор интервала
   и кнопка «Clear» над Today (⌘W, ×, ⇧⌘K и ⇧⌘T уже работают).
3. **Сохранение сессии.** Не сохраняется история «назад/вперёд» вкладок.
4. **Favorites, папки, перетаскивание и контекстные меню сайдбара.**
5. **Split View** (⌃⇧=, перетаскивание в центр).
6. **Peek, Little Arc (⌥⌘N) и Air Traffic Control.**
7. **Управление Spaces и редактор темы.** Нужны создание, переименование,
   иконка, удаление, а также градиент из нескольких цветов, наборы цветов,
   шум, Intensity и Light/Dark.
8. **Профили.** Сейчас один кэш на всё.
9. **Окно настроек**, в том числе редактор горячих клавиш с приоритетом
   «Arc или сайт».
10. **Клавиши.** Нет ⇧⌘C, ⌘F, ⌘Y, ⌘+/⌘-/⌘0, ⇧⌘L.
11. **Базовые функции браузера:** поиск по странице, масштаб, история,
    загрузки, DevTools, печать, индикатор загрузки, новое и инкогнито-окно.
12. **Медиаконтролы и PiP, тосты с Undo, онбординг и импорт.**
