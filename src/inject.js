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

    let userConfig = {
        block_ads: true,
        block_shorts: true,
        auto_confirm_watching: true,
        enable_pip: true
    };

    window.__applyConfig = function(cfg) {
        userConfig = Object.assign(userConfig, cfg);
        if (userConfig.block_shorts) {
            document.documentElement.classList.add('yt-hide-shorts');
        } else {
            document.documentElement.classList.remove('yt-hide-shorts');
        }
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

        if (userConfig.block_shorts) {
            document.documentElement.classList.add('yt-hide-shorts');
        }
    }

    // 2. Shorts Auto-Redirect to normal video player
    function checkShortsRedirect() {
        if (!userConfig.block_shorts) return;
        const path = window.location.pathname;
        if (path.startsWith('/shorts/')) {
            const videoId = path.split('/')[2];
            if (videoId) {
                window.location.replace(`/watch?v=${videoId}`);
            }
        }
    }

    // 3. Video Ad Skipper & Fast-Forward Engine
    let wasAdShowing = false;
    let savedPlaybackRate = 1.0;

    function handleVideoAds() {
        if (!userConfig.block_ads) return;

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

    // 4. Anti-Adblock Modal Remover
    function handleAntiAdblock() {
        if (!userConfig.block_ads) return;

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

    // 5. Auto-Confirm "Still Watching?"
    function handleStillWatching() {
        if (!userConfig.auto_confirm_watching) return;
        const confirmBtn = document.querySelector('yt-confirm-dialog-renderer #confirm-button button') ||
            document.querySelector('#confirm-button.yt-confirm-dialog-renderer');
        if (confirmBtn) {
            confirmBtn.click();
        }
    }

    // 6. Picture-in-Picture
    async function togglePiP() {
        if (!userConfig.enable_pip) return;
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
        if (!userConfig.enable_pip) return;
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

    // 7. Title Sync
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

    // 8. Global In-App Keyboard Shortcuts
    window.addEventListener('keydown', (e) => {
        // Cmd + , : Open Preferences / Settings
        if (e.metaKey && e.key === ',') {
            e.preventDefault();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ type: 'open_settings' }));
            }
            return;
        }
        // Cmd + Shift + P: Toggle PiP
        if (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'p') {
            e.preventDefault();
            togglePiP();
            return;
        }
        // Cmd + [ : History Back
        if (e.metaKey && e.key === '[') {
            e.preventDefault();
            window.history.back();
            return;
        }
        // Cmd + ] : History Forward
        if (e.metaKey && e.key === ']') {
            e.preventDefault();
            window.history.forward();
            return;
        }
        // Cmd + R : Reload
        if (e.metaKey && e.key.toLowerCase() === 'r') {
            e.preventDefault();
            window.location.reload();
            return;
        }
    });

    // 9. Main Loop & Observers
    function tick() {
        injectStyles();
        checkShortsRedirect();
        handleVideoAds();
        handleAntiAdblock();
        handleStillWatching();
        injectPiPButton();
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

    console.log('[yt-desktop] Clean production runtime active.');
})();
