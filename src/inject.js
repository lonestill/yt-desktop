(function () {
    'use strict';

    // 0. Forward console logs and errors to native terminal
    const _error = console.error;
    console.error = function (...args) {
        _error.apply(console, args);
        try {
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({
                    type: 'log',
                    level: 'ERROR',
                    message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ')
                }));
            }
        } catch (_) {}
    };

    const _warn = console.warn;
    console.warn = function (...args) {
        _warn.apply(console, args);
        try {
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({
                    type: 'log',
                    level: 'WARN',
                    message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ')
                }));
            }
        } catch (_) {}
    };

    // 1. Inject Styles
    function injectStyles() {
        if (document.getElementById('yt-desktop-styles')) return;
        const head = document.head || document.documentElement;
        if (!head) return;

        const style = document.createElement('style');
        style.id = 'yt-desktop-styles';
        style.textContent = window.__YT_DESKTOP_CSS__ || '';
        head.appendChild(style);
    }

    // 2. Video Ad Skipper & Fast-Forward Engine
    let wasAdShowing = false;
    let savedPlaybackRate = 1.0;

    function handleVideoAds() {
        const video = document.querySelector('video');
        const player = document.getElementById('movie_player') || document.querySelector('.html5-video-player');

        const adModule = document.querySelector('.ytp-ad-module');
        const hasAdOverlay = !!document.querySelector('.ytp-ad-player-overlay') || !!document.querySelector('.ad-showing') || !!document.querySelector('.ad-interrupting');
        const hasAdModuleChildren = adModule && adModule.children.length > 0;
        const playerHasAdClass = player && (player.classList.contains('ad-showing') || player.classList.contains('ad-interrupting'));

        const isAdActive = hasAdOverlay || hasAdModuleChildren || playerHasAdClass;

        if (isAdActive && video) {
            if (!wasAdShowing) {
                wasAdShowing = true;
                savedPlaybackRate = video.playbackRate > 2.0 ? 1.0 : video.playbackRate;
            }

            video.muted = true;
            video.playbackRate = 16.0;

            if (isFinite(video.duration) && video.duration > 0) {
                video.currentTime = video.duration;
            }

            const skipSelectors = [
                '.ytp-ad-skip-button',
                '.ytp-skip-ad-button',
                '.ytp-ad-skip-button-modern',
                '.ytp-ad-skip-button-text',
                'button.ytp-ad-skip-button-icon',
                '.ytp-ad-overlay-close-button'
            ];

            for (const sel of skipSelectors) {
                const btn = document.querySelector(sel);
                if (btn) {
                    btn.click();
                    break;
                }
            }
        } else if (wasAdShowing && video) {
            wasAdShowing = false;
            video.muted = false;
            video.playbackRate = savedPlaybackRate;
        }
    }

    // 3. Anti-Adblock Modal Remover
    function handleAntiAdblock() {
        const dialog = document.querySelector('ytd-enforcement-message-view-model') ||
            document.querySelector('tp-yt-paper-dialog:has(#feedback)');

        if (dialog) {
            const paperDialog = dialog.closest('tp-yt-paper-dialog') || dialog;
            paperDialog.remove();

            document.querySelectorAll('tp-yt-iron-overlay-backdrop').forEach(el => el.remove());
            document.body.style.overflow = 'auto';
            document.documentElement.style.overflow = 'auto';

            const video = document.querySelector('video');
            if (video && video.paused) {
                video.play().catch(() => {});
            }
        }
    }

    // 4. Auto-Confirm "Still Watching?"
    function handleStillWatching() {
        const confirmBtn = document.querySelector('yt-confirm-dialog-renderer #confirm-button button') ||
            document.querySelector('#confirm-button.yt-confirm-dialog-renderer');
        if (confirmBtn) {
            confirmBtn.click();
        }
    }

    // 5. Picture-in-Picture
    async function togglePiP() {
        const video = document.querySelector('video');
        if (!video) return;

        try {
            if (document.pictureInPictureElement) {
                await document.exitPictureInPicture();
            } else {
                await video.requestPictureInPicture();
            }
        } catch (err) {
            console.warn('[yt-desktop] PiP error:', err);
        }
    }

    function injectPiPButton() {
        if (document.getElementById('yt-desktop-pip-btn')) return;
        const rightControls = document.querySelector('.ytp-right-controls');
        if (!rightControls) return;

        const pipBtn = document.createElement('button');
        pipBtn.id = 'yt-desktop-pip-btn';
        pipBtn.className = 'ytp-button yt-desktop-pip-btn';
        pipBtn.title = 'Picture-in-Picture (Cmd+Shift+P)';
        pipBtn.innerHTML = `
            <svg viewBox="0 0 24 24">
                <path d="M19 7h-8v6h8V7zm2-4H3c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h18c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16.01H3V4.99h18v14.02z"/>
            </svg>
        `;
        pipBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            togglePiP();
        });

        const fsBtn = rightControls.querySelector('.ytp-fullscreen-button');
        if (fsBtn) {
            rightControls.insertBefore(pipBtn, fsBtn);
        } else {
            rightControls.appendChild(pipBtn);
        }
    }

    // 6. Title Sync
    let lastReportedTitle = '';
    function syncTitle() {
        const title = document.title;
        if (title && title !== lastReportedTitle) {
            lastReportedTitle = title;
            if (window.ipc) {
                try {
                    window.ipc.postMessage(JSON.stringify({ type: 'title', value: title }));
                } catch (_) {}
            }
        }
    }

    // 7. Anti-DPI State & UI System
    let dpiState = {
        port: 0,
        selected_strategy: 'auto',
        active_strategy: 'rostelecom',
        last_latency_ms: null,
        presets: []
    };
    let benchmarkResults = {};
    let isBenchmarking = false;

    function showToast(msg) {
        let toast = document.getElementById('yt-dpi-toast');
        if (!toast) {
            toast = document.createElement('div');
            toast.id = 'yt-dpi-toast';
            toast.className = 'yt-dpi-toast';
            document.body.appendChild(toast);
        }
        toast.textContent = msg;
        toast.style.display = 'block';
        clearTimeout(toast._timer);
        toast._timer = setTimeout(() => {
            toast.style.display = 'none';
        }, 3000);
    }

    function getStrategyLabel(id) {
        if (id === 'auto') return '⚡ Авто-подбор';
        const p = (dpiState.presets || []).find(x => x.id === id);
        return p ? p.name : id;
    }

    function updateTriggerBadge() {
        const trigger = document.getElementById('yt-dpi-trigger');
        if (!trigger) return;

        const activeName = getStrategyLabel(dpiState.selected_strategy === 'auto' ? dpiState.active_strategy : dpiState.selected_strategy);
        const autoPrefix = dpiState.selected_strategy === 'auto' ? '[Auto] ' : '';
        const dot = trigger.querySelector('.yt-dpi-dot');
        const text = trigger.querySelector('.yt-dpi-text');

        if (dot) {
            dot.className = isBenchmarking ? 'yt-dpi-dot busy' : 'yt-dpi-dot';
        }
        if (text) {
            text.textContent = isBenchmarking ? '⚡ Подбор...' : `⚡ ${autoPrefix}${activeName}`;
        }
    }

    window.__onDpiStatus = function (data) {
        dpiState = data;
        updateTriggerBadge();
        renderPopoverContent();
    };

    window.__onDpiBenchmarkDone = function (results, status) {
        isBenchmarking = false;
        dpiState = status;
        benchmarkResults = {};
        for (const r of results) {
            benchmarkResults[r.strategy_id] = r;
        }
        updateTriggerBadge();
        renderPopoverContent();
        const winner = results.find(r => r.strategy_id === status.active_strategy);
        if (winner && winner.success) {
            showToast(`✅ Выбрана быстрейшая стратегия: ${winner.strategy_name} (${winner.latency_ms}ms)`);
        } else {
            showToast(`⚠️ Тест завершён. Активна: ${getStrategyLabel(status.active_strategy)}`);
        }
    };

    function closePopover() {
        const popover = document.getElementById('yt-dpi-popover');
        if (popover) popover.remove();
    }

    function renderPopoverContent() {
        const popover = document.getElementById('yt-dpi-popover');
        if (!popover) return;

        const presets = dpiState.presets || [];
        const isAuto = dpiState.selected_strategy === 'auto';

        let listHtml = `
            <div class="yt-dpi-item ${isAuto ? 'active' : ''}" data-strategy="auto">
                <div class="yt-dpi-item-info">
                    <div class="yt-dpi-item-name">⚡ Авто-подбор (Рекомендуется)</div>
                    <div class="yt-dpi-item-hint">Тестирует сеть и выбирает быстрейший обход</div>
                </div>
                ${isAuto ? `<span class="yt-dpi-badge-tag fast">Активно (${dpiState.active_strategy})</span>` : ''}
            </div>
        `;

        for (const p of presets) {
            const isActive = !isAuto && dpiState.selected_strategy === p.id;
            const bench = benchmarkResults[p.id];
            let badgeHtml = '';
            if (bench) {
                if (bench.success) {
                    badgeHtml = `<span class="yt-dpi-badge-tag fast">${bench.latency_ms}ms</span>`;
                } else {
                    badgeHtml = `<span class="yt-dpi-badge-tag fail">${bench.error || 'Блок'}</span>`;
                }
            } else if (isActive) {
                badgeHtml = `<span class="yt-dpi-badge-tag">Активно</span>`;
            }

            listHtml += `
                <div class="yt-dpi-item ${isActive ? 'active' : ''}" data-strategy="${p.id}">
                    <div class="yt-dpi-item-info">
                        <div class="yt-dpi-item-name">${p.name}</div>
                        <div class="yt-dpi-item-hint">${p.isp_hint}</div>
                    </div>
                    ${badgeHtml}
                </div>
            `;
        }

        popover.innerHTML = `
            <div class="yt-dpi-popover-header">
                <div class="yt-dpi-title">🛡️ Обход блокировок (Anti-DPI)</div>
                <button class="yt-dpi-close-btn" id="yt-dpi-close-btn">&times;</button>
            </div>
            <button class="yt-dpi-bench-btn" id="yt-dpi-bench-trigger" ${isBenchmarking ? 'disabled' : ''}>
                ${isBenchmarking ? '⏳ Проверка стратегий...' : '🚀 Проверить все стратегии (Benchmark)'}
            </button>
            <div class="yt-dpi-list">
                ${listHtml}
            </div>
        `;

        document.getElementById('yt-dpi-close-btn')?.addEventListener('click', closePopover);

        document.getElementById('yt-dpi-bench-trigger')?.addEventListener('click', () => {
            if (isBenchmarking) return;
            isBenchmarking = true;
            updateTriggerBadge();
            renderPopoverContent();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ type: 'run_dpi_benchmark' }));
            }
        });

        popover.querySelectorAll('.yt-dpi-item').forEach(item => {
            item.addEventListener('click', () => {
                const strategyId = item.getAttribute('data-strategy');
                if (!strategyId) return;
                showToast(`Переключение на: ${getStrategyLabel(strategyId)}...`);
                if (window.ipc) {
                    window.ipc.postMessage(JSON.stringify({
                        type: 'set_dpi_strategy',
                        strategy_id: strategyId
                    }));
                }
            });
        });
    }

    function togglePopover() {
        const existing = document.getElementById('yt-dpi-popover');
        if (existing) {
            existing.remove();
            return;
        }

        const popover = document.createElement('div');
        popover.id = 'yt-dpi-popover';
        popover.className = 'yt-dpi-popover';
        document.body.appendChild(popover);
        renderPopoverContent();

        // Close when clicking outside
        const outsideClickListener = (e) => {
            const trigger = document.getElementById('yt-dpi-trigger');
            if (popover && !popover.contains(e.target) && (!trigger || !trigger.contains(e.target))) {
                closePopover();
                document.removeEventListener('click', outsideClickListener);
            }
        };
        setTimeout(() => document.addEventListener('click', outsideClickListener), 50);
    }

    function injectDpiTrigger() {
        if (document.getElementById('yt-dpi-trigger')) return;

        const trigger = document.createElement('button');
        trigger.id = 'yt-dpi-trigger';
        trigger.className = 'yt-dpi-trigger';
        trigger.innerHTML = `
            <span class="yt-dpi-dot"></span>
            <span class="yt-dpi-text">⚡ Anti-DPI</span>
            <span style="font-size: 10px; opacity: 0.7;">▾</span>
        `;
        trigger.addEventListener('click', (e) => {
            e.stopPropagation();
            togglePopover();
        });

        // Try placing in YouTube Masthead right controls
        const mastheadButtons = document.querySelector('ytd-masthead #end #buttons') ||
            document.querySelector('ytd-masthead #end');

        if (mastheadButtons) {
            mastheadButtons.insertBefore(trigger, mastheadButtons.firstChild);
        } else {
            // Floating fallback
            trigger.style.position = 'fixed';
            trigger.style.top = '12px';
            trigger.style.right = '80px';
            document.body.appendChild(trigger);
        }

        // Query status from Rust backend
        if (window.ipc) {
            try {
                window.ipc.postMessage(JSON.stringify({ type: 'get_dpi_status' }));
            } catch (_) {}
        }
    }

    // 8. Keyboard Shortcuts
    window.addEventListener('keydown', (e) => {
        if (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'p') {
            e.preventDefault();
            togglePiP();
            return;
        }
        if (e.metaKey && e.key === '[') {
            e.preventDefault();
            window.history.back();
            return;
        }
        if (e.metaKey && e.key === ']') {
            e.preventDefault();
            window.history.forward();
            return;
        }
        if (e.metaKey && e.key.toLowerCase() === 'r') {
            e.preventDefault();
            window.location.reload();
            return;
        }
    });

    // 9. Main Loop & Observers
    function tick() {
        injectStyles();
        handleVideoAds();
        handleAntiAdblock();
        handleStillWatching();
        injectPiPButton();
        injectDpiTrigger();
        syncTitle();
    }

    tick();
    setInterval(tick, 200);

    const observer = new MutationObserver(() => tick());
    if (document.body) {
        observer.observe(document.body, { childList: true, subtree: true });
    } else {
        document.addEventListener('DOMContentLoaded', () => {
            tick();
            if (document.body) {
                observer.observe(document.body, { childList: true, subtree: true });
            }
        });
    }

    console.log('[yt-desktop] High-performance client runtime active.');
})();
