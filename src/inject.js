(function () {
    'use strict';

    function postIpc(obj) {
        try {
            const raw = JSON.stringify(obj);
            if (window.ipc && typeof window.ipc.postMessage === 'function') {
                window.ipc.postMessage(raw);
                return true;
            }
            if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.ipc) {
                window.webkit.messageHandlers.ipc.postMessage(raw);
                return true;
            }
        } catch (_) {}
        return false;
    }

    // Trusted Types bypass policy (if supported)
    let trustedPolicy = null;
    if (window.trustedTypes && typeof window.trustedTypes.createPolicy === 'function') {
        try {
            trustedPolicy = window.trustedTypes.createPolicy('default', {
                createHTML: (s) => s,
            });
        } catch (_) {
            try {
                trustedPolicy = window.trustedTypes.createPolicy('yt-desktop-policy', {
                    createHTML: (s) => s,
                });
            } catch (_) {}
        }
    }
    const _log = console.log;
    console.log = function (...args) {
        _log.apply(console, args);
        postIpc({
            type: 'log',
            level: 'INFO',
            message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ')
        });
    };

    const _error = console.error;
    console.error = function (...args) {
        _error.apply(console, args);
        postIpc({
            type: 'log',
            level: 'ERROR',
            message: args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' ')
        });
    };

    window.addEventListener('error', function (e) {
        // Suppress generic cross-origin "Script error." and YouTube's internal kevlar errors
        if (e.message === 'Script error.' && (!e.filename || e.filename === '')) {
            return;
        }
        if (e.filename && e.filename.includes('kevlar')) {
            return;
        }
        postIpc({
            type: 'log',
            level: 'UNCAUGHT',
            message: (e.message || String(e)) + ' at ' + (e.filename || '') + ':' + (e.lineno || 0)
        });
    });

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

    function formatCount(num) {
        if (num === null || num === undefined) return '';
        if (num >= 1000000) return (num / 1000000).toFixed(1) + 'M';
        if (num >= 1000) return (num / 1000).toFixed(1) + 'K';
        return String(num);
    }

    function getActiveVideoId() {
        // 1. URL parameter (?v=...)
        try {
            const urlParams = new URLSearchParams(window.location.search);
            const v = urlParams.get('v');
            if (v && v.length === 11) return v;
        } catch (_) {}

        // 2. Pathname pattern (/shorts/..., /live/..., /watch/...)
        try {
            const match = window.location.pathname.match(/\/(shorts|live|watch)\/([a-zA-Z0-9_-]{11})/);
            if (match && match[2]) return match[2];
        } catch (_) {}

        // 3. Flexy attribute
        try {
            const flexy = document.querySelector('ytd-watch-flexy');
            if (flexy) {
                const vid = flexy.getAttribute('video-id');
                if (vid && vid.length === 11) return vid;
            }
        } catch (_) {}

        // 4. Player API
        try {
            const player = document.getElementById('movie_player');
            if (player && typeof player.getVideoData === 'function') {
                const data = player.getVideoData();
                if (data && data.video_id && data.video_id.length === 11) return data.video_id;
            }
        } catch (_) {}

        return null;
    }

    // 2. Shorts Auto-Redirect
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

    // 5. Metadata Sync with Native Rust Backend
    let requestedVideoId = null;
    let lastRequestTime = 0;
    let videoMetaCache = {};
    let lastSkippedUuid = null;

    function syncVideoMeta() {
        const videoId = getActiveVideoId();
        if (!videoId) return;

        const cached = videoMetaCache[videoId];
        const isPendingTooLong = (Date.now() - lastRequestTime) > 7000;

        if (!cached && (videoId !== requestedVideoId || isPendingTooLong)) {
            requestedVideoId = videoId;
            lastRequestTime = Date.now();
            console.log('[EXT-SYNC] 🔍 Requesting metadata for videoId: ' + videoId);
            postIpc({
                type: 'fetch_video_meta',
                video_id: videoId
            });
        }
    }

    window.__onVideoMeta = function(videoId, dislikesData, sponsorSegments) {
        try {
            console.log('[EXT-SYNC] 📥 Received meta for ' + videoId + ': dislikes=' + (dislikesData ? dislikesData.dislikes : 'none') + ', segments=' + (sponsorSegments ? sponsorSegments.length : 0));
            videoMetaCache[videoId] = {
                dislikes: dislikesData,
                segments: sponsorSegments || []
            };
            applyDislikes();
            renderSponsorSegments();
        } catch (err) {
            console.error('[EXT-SYNC] ❌ Error in __onVideoMeta: ' + err.name + ': ' + err.message + ' | ' + err.stack);
        }
    };

    // 6. Return YouTube Dislike DOM Injection
    function getDislikeButton() {
        // 1. Direct standard class / custom element
        const vm = document.querySelector('dislike-button-view-model');
        if (vm) {
            const btn = vm.querySelector('button');
            if (btn) return btn;
        }

        // 2. Segmented dislike button ID
        const seg = document.querySelector('#segmented-dislike-button button, segmented-like-dislike-button-view-model dislike-button-view-model button');
        if (seg) return seg;

        // 3. Segmented-end class
        const segEnd = document.querySelector('.yt-spec-button-shape-next--segmented-end');
        if (segEnd) return segEnd;

        // 4. Renderer last child
        const renderer = document.querySelector('ytd-segmented-like-dislike-button-renderer');
        if (renderer) {
            const btns = renderer.querySelectorAll('button');
            if (btns.length >= 2) return btns[btns.length - 1];
        }

        // 5. Semantic aria-label fallback (Russian & English)
        const ariaBtn = document.querySelector('button[aria-label*="не нравится" i], button[aria-label*="dislike" i]');
        if (ariaBtn) return ariaBtn;

        return null;
    }

    function getLikeButton() {
        const vm = document.querySelector('like-button-view-model');
        if (vm) {
            const btn = vm.querySelector('button');
            if (btn) return btn;
        }
        const container = document.querySelector('segmented-like-dislike-button-view-model, ytd-segmented-like-dislike-button-renderer');
        if (container) {
            const btn = container.querySelector('button');
            if (btn) return btn;
        }
        const ariaBtn = document.querySelector('button[aria-label*="нравится" i]:not([aria-label*="не нравится" i]), button[aria-label*="like" i]:not([aria-label*="dislike" i])');
        if (ariaBtn) return ariaBtn;

        return null;
    }

    function applyDislikes() {
        try {
            if (!userConfig.return_dislikes) return;
            const videoId = getActiveVideoId();
            if (!videoId) return;

            const meta = videoMetaCache[videoId];
            if (!meta || !meta.dislikes) return;

            const dislikes = meta.dislikes.dislikes;
            const likes = meta.dislikes.likes;
            if (dislikes === undefined || dislikes === null) return;

            const dislikeBtn = getDislikeButton();
            if (dislikeBtn) {
                let label = dislikeBtn.querySelector('.yt-ryd-label');
                if (!label) {
                    const likeBtn = getLikeButton();
                    const likeText = likeBtn ? likeBtn.querySelector('.yt-spec-button-shape-next__button-text-content') : null;

                    if (likeText) {
                        label = likeText.cloneNode(false);
                        label.className = likeText.className + ' yt-ryd-label';
                    } else {
                        label = document.createElement('div');
                        label.className = 'yt-spec-button-shape-next__button-text-content yt-ryd-label';
                    }

                    const span = document.createElement('span');
                    span.setAttribute('role', 'text');
                    span.textContent = formatCount(dislikes);
                    label.replaceChildren(span);

                    // Safe insertion: only use insertBefore if feedback is an IMMEDIATE child
                    const feedback = dislikeBtn.querySelector('yt-touch-feedback-shape');
                    if (feedback && feedback.parentNode === dislikeBtn) {
                        dislikeBtn.insertBefore(label, feedback);
                    } else {
                        dislikeBtn.appendChild(label);
                    }
                    console.log(`[RYD-DISLIKE] ✅ Injected dislike count (${formatCount(dislikes)}) into dislike button`);
                } else {
                    const span = label.querySelector('span');
                    const text = formatCount(dislikes);
                    if (span && span.textContent !== text) {
                        span.textContent = text;
                    } else if (!span && label.textContent !== text) {
                        label.textContent = text;
                    }
                }

                dislikeBtn.classList.remove('yt-spec-button-shape-next--icon-only');
                dislikeBtn.classList.add('yt-spec-button-shape-next--icon-leading');
                dislikeBtn.style.setProperty('width', 'auto', 'important');
                dislikeBtn.style.setProperty('min-width', '52px', 'important');
                dislikeBtn.style.setProperty('overflow', 'visible', 'important');
            }

            // Ratio Bar under like/dislike segmented container
            const buttonContainer = document.querySelector('segmented-like-dislike-button-view-model') ||
                document.querySelector('ytd-segmented-like-dislike-button-renderer') ||
                document.querySelector('like-button-view-model')?.parentElement?.parentElement;

            if (buttonContainer && (likes !== undefined && dislikes !== undefined)) {
                buttonContainer.style.position = 'relative';
                let bar = buttonContainer.querySelector('#yt-ryd-ratio-bar');
                if (!bar) {
                    bar = document.createElement('div');
                    bar.id = 'yt-ryd-ratio-bar';
                    bar.className = 'yt-ryd-ratio-bar';
                    buttonContainer.appendChild(bar);
                }

                const total = (likes || 0) + (dislikes || 0);
                const likePct = total > 0 ? ((likes / total) * 100).toFixed(1) : 100;
                let fill = bar.querySelector('.yt-ryd-likes-fill');
                if (!fill) {
                    fill = document.createElement('div');
                    fill.className = 'yt-ryd-likes-fill';
                    bar.appendChild(fill);
                }
                fill.style.width = `${likePct}%`;
            }
        } catch (err) {
            console.error('[RYD-DISLIKE] ❌ applyDislikes error: ' + err.name + ': ' + err.message + ' | ' + err.stack);
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
        const prefix = document.createElement('span');
        prefix.textContent = '⏩ Пропущено: ';
        const catBold = document.createElement('b');
        catBold.textContent = labelMap[category] || category;
        toast.replaceChildren(prefix, catBold);
        toast.style.display = 'flex';
        clearTimeout(toast._timer);
        toast._timer = setTimeout(() => { toast.style.display = 'none'; }, 2200);
    }

    function checkSponsorSkip() {
        try {
            if (!userConfig.sponsor_block) return;
            const videoId = getActiveVideoId();
            if (!videoId) return;

            const meta = videoMetaCache[videoId];
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
                        console.log(`[SPONSOR-BLOCK] ⏩ Auto-skipped ${seg.category} (${start.toFixed(1)}s -> ${end.toFixed(1)}s)`);
                        break;
                    }
                }
            }
        } catch (err) {
            console.error('[SPONSOR-BLOCK] ❌ checkSponsorSkip error: ' + (err.stack || err.message || String(err)));
        }
    }

    function getProgressBar() {
        return document.querySelector('.ytp-progress-bar') ||
               document.querySelector('.ytChapteredProgressBarHost') ||
               document.querySelector('.YtProgressBarLineHost') ||
               document.querySelector('.ytp-progress-list');
    }

    function renderSponsorSegments() {
        try {
            if (!userConfig.sponsor_block) return;
            const videoId = getActiveVideoId();
            if (!videoId) return;

            const meta = videoMetaCache[videoId];
            if (!meta || !meta.segments || !meta.segments.length) return;

            const progressBar = getProgressBar();
            const video = document.querySelector('video');
            if (!progressBar) return;
            if (!video || !isFinite(video.duration) || video.duration <= 0) return;

            let container = progressBar.querySelector('#yt-sb-previewbar');
            if (!container) {
                container = document.createElement('div');
                container.id = 'yt-sb-previewbar';
                progressBar.insertBefore(container, progressBar.firstChild);
            }

            if (container.dataset.renderedVideoId === videoId && container.children.length === meta.segments.length) {
                return;
            }

            container.replaceChildren();
            container.dataset.renderedVideoId = videoId;
            const duration = video.duration;

            for (const seg of meta.segments) {
                const [start, end] = seg.segment;
                const leftPct = Math.max(0, Math.min(100, (start / duration) * 100));
                const widthPct = Math.max(0.2, Math.min(100 - leftPct, ((end - start) / duration) * 100));

                const marker = document.createElement('div');
                marker.className = `yt-sb-segment ${seg.category}`;
                marker.style.left = `${leftPct}%`;
                marker.style.width = `${widthPct}%`;
                container.appendChild(marker);
            }
            console.log(`[SPONSOR-BLOCK] 🎨 Rendered ${meta.segments.length} segment markers on timeline (${videoId})`);
        } catch (err) {
            console.error('[SPONSOR-BLOCK] ❌ renderSponsorSegments error: ' + err.name + ': ' + err.message + ' | ' + err.stack);
        }
    }

    function hookVideoEvents() {
        const video = document.querySelector('video');
        if (video && !video.__sbHooked) {
            video.__sbHooked = true;
            video.addEventListener('timeupdate', checkSponsorSkip);
            video.addEventListener('durationchange', renderSponsorSegments);
            video.addEventListener('loadedmetadata', renderSponsorSegments);
        }
    }

    // 8. Max Quality Enforcer
    function enforceMaxQuality() {
        if (!userConfig.max_quality) return;
        try {
            const player = document.getElementById('movie_player');
            if (player && typeof player.getPlaybackQuality === 'function' && typeof player.setPlaybackQualityRange === 'function') {
                const current = player.getPlaybackQuality();
                if (current && (current === 'tiny' || current === 'small' || current === 'medium' || current === 'large')) {
                    player.setPlaybackQualityRange('highres');
                }
            }
        } catch (_) {}
    }

    // 9. Auto-Confirm "Still Watching?"
    function handleStillWatching() {
        if (!userConfig.auto_confirm_watching) return;
        try {
            const confirmBtn = document.querySelector('yt-confirm-dialog-renderer #confirm-button button') ||
                document.querySelector('#confirm-button.yt-confirm-dialog-renderer');
            if (confirmBtn) {
                confirmBtn.click();
            }
        } catch (_) {}
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
        const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
        svg.setAttribute('viewBox', '0 0 24 24');
        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        path.setAttribute('d', 'M19 7h-8v6h8V7zm2-4H3c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h18c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zm0 16.01H3V4.99h18v14.02z');
        svg.appendChild(path);
        pipBtn.replaceChildren(svg);
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
        try {
            const title = document.title;
            if (title && title !== lastReportedTitle) {
                lastReportedTitle = title;
                postIpc({ type: 'title', value: title });
            }
        } catch (_) {}
    }

    // 12. Global Keyboard Shortcuts
    window.addEventListener('keydown', (e) => {
        if (e.metaKey && e.key === ',') {
            e.preventDefault();
            postIpc({ type: 'open_settings' });
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

    // SPA Navigation Listeners
    window.addEventListener('yt-navigate-finish', () => {
        syncVideoMeta();
        tick();
    });
    window.addEventListener('yt-page-data-updated', () => {
        syncVideoMeta();
        tick();
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
        hookVideoEvents();

        syncVideoMeta();
        applyDislikes();
        renderSponsorSegments();
        checkSponsorSkip();
    }

    tick();
    setInterval(tick, 150);

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

    console.log('[yt-desktop] High-speed extensions engine initialized.');
})();
