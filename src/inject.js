(function () {
    'use strict';

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

            // Mute ad and fast-forward at 16x
            video.muted = true;
            video.playbackRate = 16.0;

            if (isFinite(video.duration) && video.duration > 0) {
                video.currentTime = video.duration;
            }

            // Click skip buttons
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

    // 6. Keyboard Shortcuts
    window.addEventListener('keydown', (e) => {
        // Cmd + Shift + P: Toggle PiP
        if (e.metaKey && e.shiftKey && e.key.toLowerCase() === 'p') {
            e.preventDefault();
            togglePiP();
            return;
        }
        // Cmd + [: Back
        if (e.metaKey && e.key === '[') {
            e.preventDefault();
            window.history.back();
            return;
        }
        // Cmd + ]: Forward
        if (e.metaKey && e.key === ']') {
            e.preventDefault();
            window.history.forward();
            return;
        }
        // Cmd + R: Reload
        if (e.metaKey && e.key.toLowerCase() === 'r') {
            e.preventDefault();
            window.location.reload();
            return;
        }
    });

    // 7. Title Sync to Native macOS Window
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

    // 8. Main Loop & Observers
    function tick() {
        injectStyles();
        handleVideoAds();
        handleAntiAdblock();
        handleStillWatching();
        injectPiPButton();
        syncTitle();
    }

    // Execute immediately and periodically
    tick();
    setInterval(tick, 200);

    // Watch DOM mutations for instant response to SPA navigation
    const observer = new MutationObserver(() => {
        tick();
    });

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
