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

    let userConfig = {
        block_ads: true,
        sponsor_block: true,
        return_dislikes: true,
        block_shorts: true,
        auto_confirm_watching: true,
        enable_pip: true,
        oled_black: false,
        max_quality: true
    };

    window.__applyConfig = function(cfg) {
        userConfig = Object.assign(userConfig, cfg);
        if (userConfig.block_shorts) {
            document.documentElement.classList.add('yt-hide-shorts');
        } else {
            document.documentElement.classList.remove('yt-hide-shorts');
        }
        if (userConfig.oled_black) {
            document.documentElement.classList.add('yt-oled-black');
        } else {
            document.documentElement.classList.remove('yt-oled-black');
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
        if (userConfig.oled_black) {
            document.documentElement.classList.add('yt-oled-black');
        }
    }

    // Helper: format numbers (e.g. 14200 -> 14.2K)
    function formatCount(num) {
        if (!num && num !== 0) return '';
        if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
        if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
        return String(num);
    }

    // Helper: extract active Video ID from URL
    function getActiveVideoId() {
        const urlParams = new URLSearchParams(window.location.search);
        return urlParams.get('v');
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

    // 5. Metadata Cache & Communication with Rust
    let activeVideoId = null;
    let videoMetaCache = {};
    let lastSkippedUuid = null;

    function requestVideoMetaIfNeeded() {
        const videoId = getActiveVideoId();
        if (!videoId) return;

        if (videoId !== activeVideoId) {
            activeVideoId = videoId;
            lastSkippedUuid = null;

            // Trigger fetch in Rust backend if not cached
            if (!videoMetaCache[videoId] && window.ipc) {
                window.ipc.postMessage(JSON.stringify({
                    type: 'fetch_video_meta',
                    video_id: videoId
                }));
            }
        }
    }

    // Rust delivers fetched metadata here
    window.__onVideoMeta = function(videoId, dislikesData, sponsorSegments) {
        videoMetaCache[videoId] = {
            dislikes: dislikesData,
            segments: sponsorSegments || []
        };
        applyDislikes(videoId);
        renderSponsorSegments(videoId);
    };

    // 6. Return YouTube Dislike DOM Injection
    function applyDislikes(targetVideoId) {
        if (!userConfig.return_dislikes) return;
        const meta = videoMetaCache[targetVideoId || activeVideoId];
        if (!meta || !meta.dislikes) return;

        const dislikes = meta.dislikes.dislikes;
        const likes = meta.dislikes.likes;

        // Try multiple selectors across YouTube layout variations
        const dislikeBtn = document.querySelector('dislike-button-view-model button') ||
            document.querySelector('#segmented-dislike-button button') ||
            document.querySelector('ytd-segmented-like-dislike-button-renderer button:last-child') ||
            document.querySelector('.yt-spec-button-shape-next--segmented-end');

        if (dislikeBtn) {
            let label = dislikeBtn.querySelector('.yt-ryd-label');
            if (!label) {
                label = document.createElement('span');
                label.className = 'yt-ryd-label';
                dislikeBtn.appendChild(label);
            }
            label.textContent = formatCount(dislikes);
        }

        // Like/Dislike Ratio Bar
        const buttonContainer = document.querySelector('ytd-segmented-like-dislike-button-renderer') ||
            document.querySelector('like-button-view-model')?.parentElement?.parentElement;

        if (buttonContainer && !document.getElementById('yt-ryd-ratio-bar')) {
            const total = (likes || 0) + (dislikes || 0);
            const likePct = total > 0 ? ((likes / total) * 100).toFixed(1) : 100;

            const bar = document.createElement('div');
            bar.id = 'yt-ryd-ratio-bar';
            bar.className = 'yt-ryd-ratio-bar';
            bar.innerHTML = `<div class="yt-ryd-likes-fill" style="width: ${likePct}%;"></div>`;
            buttonContainer.appendChild(bar);
        }
    }

    // 7. SponsorBlock Auto-Skip & UI Markers
    function showSponsorToast(category) {
        let toast = document.getElementById('yt-sb-toast');
        if (!toast) {
            toast = document.createElement('div');
            toast.id = 'yt-sb-toast';
            toast.className = 'yt-sb-toast';
            const player = document.getElementById('movie_player') || document.body;
            player.appendChild(toast);
        }
        const labelMap = {
            sponsor: 'Спонсорская интеграция',
            selfpromo: 'Самореклама / подписка',
            interaction: 'Просьба лайка / подписки',
            intro: 'Интро / заставка',
            outro: 'Аутро'
        };
        toast.innerHTML = `⏩ Пропущено: <b>${labelMap[category] || category}</b>`;
        toast.style.display = 'flex';
        clearTimeout(toast._timer);
        toast._timer = setTimeout(() => { toast.style.display = 'none'; }, 2200);
    }

    function checkSponsorSkip() {
        if (!userConfig.sponsor_block) return;
        const meta = videoMetaCache[activeVideoId];
        if (!meta || !meta.segments || !meta.segments.length) return;

        const video = document.querySelector('video');
        if (!video || video.paused) return;

        const ct = video.currentTime;
        for (const seg of meta.segments) {
            const [start, end] = seg.segment;
            if (ct >= start && ct < (end - 0.2)) {
                if (lastSkippedUuid !== seg.UUID) {
                    lastSkippedUuid = seg.UUID;
                    video.currentTime = end;
                    showSponsorToast(seg.category);
                    break;
                }
            }
        }
    }

    function renderSponsorSegments(targetVideoId) {
        const meta = videoMetaCache[targetVideoId || activeVideoId];
        if (!meta || !meta.segments) return;

        document.querySelectorAll('.yt-sb-segment').forEach(el => el.remove());
        const progressBar = document.querySelector('.ytp-progress-bar');
        const video = document.querySelector('video');
        if (!progressBar || !video || !video.duration || !meta.segments.length) return;

        const duration = video.duration;
        for (const seg of meta.segments) {
            const [start, end] = seg.segment;
            const leftPct = (start / duration) * 100;
            const widthPct = ((end - start) / duration) * 100;

            const marker = document.createElement('div');
            marker.className = `yt-sb-segment ${seg.category}`;
            marker.style.left = `${leftPct}%`;
            marker.style.width = `${widthPct}%`;
            progressBar.appendChild(marker);
        }
    }

    // 8. Max Quality Enforcer
    function enforceMaxQuality() {
        if (!userConfig.max_quality) return;
        const player = document.getElementById('movie_player');
        if (player && typeof player.getPlaybackQuality === 'function' && typeof player.setPlaybackQualityRange === 'function') {
            const current = player.getPlaybackQuality();
            if (current && (current === 'tiny' || current === 'small' || current === 'medium' || current === 'large')) {
                player.setPlaybackQualityRange('highres');
            }
        }
    }

    // 9. Auto-Confirm "Still Watching?"
    function handleStillWatching() {
        if (!userConfig.auto_confirm_watching) return;
        const confirmBtn = document.querySelector('yt-confirm-dialog-renderer #confirm-button button') ||
            document.querySelector('#confirm-button.yt-confirm-dialog-renderer');
        if (confirmBtn) {
            confirmBtn.click();
        }
    }

    // 10. Picture-in-Picture
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
        } catch (_) {}
    }

    function injectPiPButton() {
        if (!userConfig.enable_pip || document.getElementById('yt-desktop-pip-btn')) return;
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

    // 11. Title Sync
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

    // 12. Global Keyboard Shortcuts
    window.addEventListener('keydown', (e) => {
        if (e.metaKey && e.key === ',') {
            e.preventDefault();
            if (window.ipc) {
                window.ipc.postMessage(JSON.stringify({ type: 'open_settings' }));
            }
            return;
        }
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

    // 13. Main Loop & Observers
    function tick() {
        injectStyles();
        checkShortsRedirect();
        handleVideoAds();
        handleAntiAdblock();
        handleStillWatching();
        injectPiPButton();
        syncTitle();
        enforceMaxQuality();

        requestVideoMetaIfNeeded();
        if (activeVideoId) {
            applyDislikes(activeVideoId);
            renderSponsorSegments(activeVideoId);
            checkSponsorSkip();
        }
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

    console.log('[yt-desktop] SponsorBlock, ReturnDislike & Clean layout active.');
})();
