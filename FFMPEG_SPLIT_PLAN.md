## Plan: Split FFmpeg Into Core Lib + Reusable GUI

Цель: разделить текущий ffmpeg-модуль на переиспользуемую библиотеку и GUI-слой так, чтобы один и тот же egui UI можно было встраивать как в standalone приложение, так и в REAPER extension frontend. Первый публичный API библиотеки — высокоуровневые операции (trim/remux/replace-audio/reencode). Политика copy/null — ручной контроль пользователем без автоматического fallback.

**Steps**
1. Phase 1 — Boundary Mapping and Contracts
1.1. Зафиксировать границы между REAPER-спецификой и portable кодом: что остается в плагине (чтение проекта, ExtState, action wiring), что уходит в core.
1.2. Формализовать модели задач рендера как независимые от REAPER структуры: source inputs, trims, audio replacement, output settings, filter chain.
1.3. Определить контракты для GUI-хостинга: UI не запускает процессы напрямую, а эмитит команды/интенты в host callbacks. Это позволяет одинаково хостить UI в standalone и в REAPER frontend.
1.4. Зафиксировать backward compatibility: старый socket-протокол временно сохраняется как адаптер вокруг новых core-операций.

2. Phase 2 — Core Library Design (high-level API first) (*depends on 1*)
2.1. Спроектировать crate `ffmpeg_core` (рабочее имя):
- модель операций `TrimClip`, `ReplaceAudio`, `RemuxCopy`, `ReencodeClip`, `BatchPlan`;
- validator совместимости для ручных copy/null режимов (отдельный шаг проверки, без авто-fallback);
- command planner, генерирующий детерминированный план ffmpeg-команд и аргументов.
2.2. Выделить из текущего кода portable сущности в core (settings/options/filter metadata/parser/stream ids) через migration wrappers.
2.3. Добавить capability service (чтение/кэш парсинга ffmpeg) как зависимость core API, чтобы UI мог ограничивать доступные опции.
2.4. Ввести типизированные ошибки по операциям (incompatible copy, missing stream, invalid trim bounds, unsupported muxer).

3. Phase 3 — Reusable Egui UI Factory (*depends on 1, parallel with late 2*)
3.1. Спроектировать crate `ffmpeg_ui` (рабочее имя) с публичной функцией/фасадом, возвращающим egui UI state + events вместо прямого запуска ffmpeg.
3.2. Разделить UI на:
- pure view/state reducers;
- host bridge trait (операции: выбрать источник/выход, запустить batch, отменить job, получить прогресс, получить capabilities).
3.3. Вынести текущие render settings/filter widgets в UI crate с минимальными зависимостями от REAPER.
3.4. Добавить явные UI-переключатели кодеков `copy` и `null` в ручном режиме, с предупреждениями о совместимости, но без silent fallback.

4. Phase 4 — Execution Engine With Controlled Concurrency (*depends on 2,3*)
4.1. Перенести запуск ffmpeg-процессов в отдельный execution модуль (в core или host-level adapter, по факту зависимостей).
4.2. Реализовать очередь с лимитом одновременных инстансов (`max_concurrent_jobs`) вместо unbounded spawn.
4.3. Поддержать режимы: serial, limited parallel, cancel/retry; унифицировать прогресс-события для UI.
4.4. Обеспечить одинаковое поведение исполнения для standalone host и REAPER host (общий scheduler контракт).

5. Phase 5 — Host Integrations
5.1. REAPER integration path (*depends on 2,3,4*):
- оставить timeline slicing в REAPER стороне;
- конвертировать timeline в high-level core operations;
- использовать `ffmpeg_ui` через host bridge вместо тесной связки со старым backend state.
5.2. Standalone app path (*depends on 2,3,4*):
- минимальный host для `ffmpeg_ui` без file-picker как hard requirement;
- входные данные могут приходить из drag-and-drop/аргументов/внешнего launcher позже;
- фокус на том же UI+queue+executor стекe.
5.3. Временный compatibility layer: существующий `ffmpeg_front`/socket оставить до полной миграции экранов и state.

6. Phase 6 — Migration and Cleanup (*depends on 5*)
6.1. Пошагово переключить существующие действия на новые core operations.
6.2. Вычистить дублирующие структуры состояния и лишние связи с REAPER в UI.
6.3. Решить судьбу `nodes.rs`: либо удалить как неиспользуемый, либо оформить как roadmap для node-graph режима.

**Relevant files**
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/base.rs — текущее построение команд и timeline-дерево; источник для extraction в core planner.
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/base_types.rs — RenderSettings/типы настроек для миграции в core models.
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/options.rs — metadata опций/кодеков для shared capability model.
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/parser.rs — parsing ffmpeg capabilities; кандидат на core service.
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/filters.rs — типы фильтров для shared UI/core.
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/gui/mod.rs — текущий backend/frontend state и socket bridge; зона адаптера на период миграции.
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/gui/render_widget.rs — текущий spawn/progress слой; источник для scheduler extraction.
- /home/levitanus/gits/reaper-levitanus/front/src/main.rs — host для GUI; кандидат на подключение `ffmpeg_ui` фасада.
- /home/levitanus/gits/reaper-levitanus/plugin/src/lib.rs — REAPER action entrypoints; wiring новых host adapters.
- /home/levitanus/gits/reaper-levitanus/reaper-levitanus/src/ffmpeg/mod.rs — текущая сборка модуля; точка реэкспорта новых crates.

**Verification**
1. Contract tests core API: для каждого high-level operation проверяется план аргументов ffmpeg и корректная ошибка при несовместимости.
2. Scheduler tests: лимит `max_concurrent_jobs` строго соблюдается; serial/parallel/cancel сценарии стабильны.
3. UI-host integration tests: один и тот же `ffmpeg_ui` работает в двух host-реализациях (REAPER adapter и standalone adapter).
4. Regression tests REAPER path: текущий workflow экспорта/рендера остается рабочим после подключения core.
5. Manual acceptance for user cases:
- fast trim клипа + замена на студийный звук;
- замена аудиодорожки в готовом видео без изменения видеопотока;
- ручной выбор `copy/null` и предсказуемая ошибка при несовместимости.

**Decisions**
- Принято: приоритет — split на library + standalone GUI.
- Принято: публичный API сначала high-level operations.
- Принято: UI поставляется как фабрика/фасад egui-компонентов для двух host-окружений.
- Принято: `copy/null` режимы — ручной контроль без автоматического fallback на re-encode.

**Scope Boundaries**
- Включено: архитектурное разделение, high-level API, общий UI-слой, контролируемая очередь выполнения.
- Исключено на первой итерации: полноценный file-picker как обязательная часть, расширение OTIO-фич, новый node-graph editor.

**Further Considerations**
1. Хранение batch-проектов: JSON schema для кросс-хоста (REAPER/standalone) стоит зафиксировать до массовой миграции.
2. Политику copy-совместимости лучше сделать явной в UI (предпроверка + reason), чтобы избежать неочевидных ffmpeg ошибок.
3. На этапе миграции полезно сохранить режим «legacy renderer» toggle для безопасного отката.
