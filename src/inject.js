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
        if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
        if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
        return String(num);
    }

    // Helper: extract active Video ID
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

    // 5. SponsorBlock Engine
    let currentSponsorVideoId = null;
    let currentSponsorSegments = [];
    let lastSkippedSegmentId = null;

    async function loadSponsorSegments(videoId) {
        if (currentSponsorVideoId === videoId) return;
        currentSponsorVideoId = videoId;
        currentSponsorSegments = [];
        lastSkippedSegmentId = null;

        if (!userConfig.sponsor_block || !videoId) return;

        try {
            const res = await fetch(`https://sponsor.ajay.app/api/skipSegments?videoID=${videoId}&categories=["sponsor","selfpromo","interaction","intro","outro"]`);
            if (res.ok) {
                const data = await res.json();
                currentSponsorSegments = data;
                renderSponsorSegmentsOnBar();
            }
        } catch (_) {}
    }

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
        toast._timer = setTimeout(() => { toast.style.display = 'none'; }, 2500);
    }

    function checkSponsorSkip() {
        if (!userConfig.sponsor_block || currentSponsorSegments.length === 0) return;
        const video = document.querySelector('video');
        if (!video || video.paused) return;

        const time = video.currentTime;
        for (const seg of currentSponsorSegments) {
            const [start, end] = seg.segment;
            if (time >= start && time < (end - 0.2)) {
                if (lastSkippedSegmentId !== seg.UUID) {
                    lastSkippedSegmentId = seg.UUID;
                    video.currentTime = end;
                    showSponsorToast(seg.category);
                    break;
                }
            }
        }
    }

    function renderSponsorSegmentsOnBar() {
        document.querySelectorAll('.yt-sb-segment').forEach(el => el.remove());
        const progressBar = document.querySelector('.ytp-progress-bar');
        const video = document.querySelector('video');
        if (!progressBar || !video || !video.duration || currentSponsorSegments.length === 0) return;

        const duration = video.duration;
        for (const seg of currentSponsorSegments) {
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

    // 6. Return YouTube Dislike Engine
    let currentDislikeVideoId = null;

    async function loadDislikes(videoId) {
        if (currentDislikeVideoId === videoId || !userConfig.return_dislikes || !videoId) return;
        currentDislikeVideoId = videoId;

        try {
            const res = await fetch(`https://returnyoutubedislikeapi.com/votes?videoId=${videoId}`);
            if (res.ok) {
                const data = await res.json();
                renderDislikes(data);
            }
        } catch (_) {}
    }

    function renderDislikes(data) {
        if (!data || !userConfig.return_dislikes) return;

        const dislikeBtn = document.querySelector('dislike-button-view-model button') ||
            document.querySelector('#segmented-dislike-button button') ||
            document.querySelector('.yt-spec-button-shape-next--segmented-end');

        if (dislikeBtn) {
            let labelSpan = dislikeBtn.querySelector('.yt-ryd-label');
            if (!labelSpan) {
                labelSpan = document.createElement('span');
                labelSpan.className = 'yt-ryd-label';
                labelSpan.style.marginLeft = '6px';
                labelSpan.style.fontSize = '12px';
                dislikeBtn.appendChild(labelSpan);
            }
            labelSpan.textContent = formatCount(data.dislikes);
        }

        // Like / Dislike ratio bar
        const buttonGroup = document.querySelector('ytd-segmented-like-dislike-button-renderer') ||
            document.querySelector('like-button-view-model')?.parentElement;

        if (buttonGroup && !document.getElementById('yt-ryd-ratio-bar')) {
            const total = (data.likes || 0) + (data.dislikes || 0);
            const likePct = total > 0 ? ((data.likes / total) * 100).toFixed(1) : 100;

            const bar = document.createElement('div');
            bar.id = 'yt-ryd-ratio-bar';
            bar.className = 'yt-ryd-ratio-bar';
            bar.innerHTML = `<div class="yt-ryd-likes-fill" style="width: ${likePct}%;"></div>`;
            buttonGroup.appendChild(bar);
        }
    }

    // 7. Max Quality Enforcer
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

    // 8. Auto-Confirm "Still Watching?"
    function handleStillWatching() {
        if (!userConfig.auto_confirm_watching) return;
        const confirmBtn = document.querySelector('yt-confirm-dialog-renderer #confirm-button button') ||
            document.querySelector('#confirm-button.yt-confirm-dialog-renderer');
        if (confirmBtn) {
            confirmBtn.click();
        }
    }

    // 9. Picture-in-Picture
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

    // 10. Title Sync
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

    // 11. Global Keyboard Shortcuts
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

    // 12. Main Loop & Observers
    function tick() {
        injectStyles();
        checkShortsRedirect();
        handleVideoAds();
        handleAntiAdblock();
        handleStillWatching();
        injectPiPButton();
        syncTitle();
        enforceMaxQuality();

        const videoId = getActiveVideoId();
        if (videoId) {
            loadSponsorSegments(videoId);
            loadDislikes(videoId);
            checkSponsorSkip();
        }
    }

    tick();
    setInterval(tick, 250);

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
