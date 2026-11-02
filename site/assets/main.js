// BananaV's website: the theme toggle, the menu on small screens and the
// sections appearing as they scroll into view. Nothing here is needed to read
// the page.
(function () {
  var root = document.documentElement;
  root.classList.add("js");

  // The theme: the one picked last, else the system's.
  function stored() {
    try { return localStorage.getItem("bananav-theme"); } catch (e) { return null; }
  }
  if (!stored() && window.matchMedia && matchMedia("(prefers-color-scheme: light)").matches) {
    root.dataset.theme = "light";
  }
  var toggle = document.querySelector(".theme-toggle");
  if (toggle) {
    toggle.addEventListener("click", function () {
      var next = root.dataset.theme === "light" ? "dark" : "light";
      root.dataset.theme = next;
      try { localStorage.setItem("bananav-theme", next); } catch (e) {}
    });
  }

  // The menu on small screens.
  var menu = document.querySelector(".menu-toggle");
  var links = document.getElementById("nav-links");
  if (menu && links) {
    menu.addEventListener("click", function () {
      var open = links.classList.toggle("open");
      menu.setAttribute("aria-expanded", open ? "true" : "false");
    });
    links.addEventListener("click", function (e) {
      if (e.target.closest("a")) {
        links.classList.remove("open");
        menu.setAttribute("aria-expanded", "false");
      }
    });
  }

  // The language: ?lang= in the address, else the one picked last, else the
  // browser's (Portuguese for any "pt"), else English. The page is written in
  // English; other languages replace its texts from i18n.js, and English puts
  // them back.
  var tables = window.BANANAV_I18N || {};
  var squash = function (s) { return s.replace(/\s+/g, " ").trim(); };
  var texts = [];
  var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
    acceptNode: function (n) {
      var p = n.parentNode && n.parentNode.nodeName;
      if (p === "SCRIPT" || p === "STYLE" || !squash(n.nodeValue)) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    }
  });
  while (walker.nextNode()) {
    var n = walker.currentNode;
    texts.push({ node: n, original: n.nodeValue, key: squash(n.nodeValue) });
  }
  var attrs = [];
  document.querySelectorAll("[alt], [aria-label], [title], meta[name='description']").forEach(function (el) {
    ["alt", "aria-label", "title", "content"].forEach(function (a) {
      var v = el.getAttribute(a);
      if (v && (a !== "content" || el.nodeName === "META")) attrs.push({ el: el, attr: a, original: v });
    });
  });
  var title = document.title;
  var langCode = document.querySelector(".lang-code");
  var langButtons = document.querySelectorAll(".lang-menu [data-lang]");

  function apply(lang) {
    var table = tables[lang] || {};
    texts.forEach(function (t) {
      var to = table[t.key];
      if (!to) { t.node.nodeValue = t.original; return; }
      var lead = t.original.match(/^\s*/)[0], tail = t.original.match(/\s*$/)[0];
      t.node.nodeValue = lead + to + tail;
    });
    attrs.forEach(function (a) { a.el.setAttribute(a.attr, table[a.original] || a.original); });
    document.title = table[title] || title;
    root.lang = lang;
    if (langCode) langCode.textContent = lang === "en" ? "EN" : lang.slice(0, 2).toUpperCase();
    langButtons.forEach(function (b) { b.setAttribute("aria-checked", b.dataset.lang === lang ? "true" : "false"); });
  }

  function known(lang) { return lang === "en" || tables.hasOwnProperty(lang); }
  function pick() {
    var asked = new URLSearchParams(location.search).get("lang");
    if (asked && known(asked)) return asked;
    var saved = null;
    try { saved = localStorage.getItem("bananav-lang"); } catch (e) {}
    if (saved && known(saved)) return saved;
    var mine = (navigator.languages || [navigator.language || "en"]);
    for (var i = 0; i < mine.length; i++) {
      if (/^pt/i.test(mine[i])) return "pt-BR";
      if (/^en/i.test(mine[i])) return "en";
    }
    return "en";
  }
  apply(pick());

  var langToggle = document.querySelector(".lang-toggle");
  var langMenu = document.getElementById("lang-menu");
  function showMenu(open) {
    if (!langMenu) return;
    langMenu.hidden = !open;
    langToggle.setAttribute("aria-expanded", open ? "true" : "false");
  }
  if (langToggle && langMenu) {
    langToggle.addEventListener("click", function (e) {
      e.stopPropagation();
      showMenu(langMenu.hidden);
    });
    langButtons.forEach(function (b) {
      b.addEventListener("click", function () {
        apply(b.dataset.lang);
        try { localStorage.setItem("bananav-lang", b.dataset.lang); } catch (e) {}
        showMenu(false);
      });
    });
    document.addEventListener("click", function (e) {
      if (!e.target.closest(".lang")) showMenu(false);
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape") showMenu(false);
    });
  }

  // The tour's tabs: one picture at a time; arrows move between the tabs.
  var tabs = Array.prototype.slice.call(document.querySelectorAll(".tour-tabs [role='tab']"));
  function selectTab(tab, focus) {
    tabs.forEach(function (t) {
      var on = t === tab;
      t.setAttribute("aria-selected", on ? "true" : "false");
      t.tabIndex = on ? 0 : -1;
      var panel = document.getElementById(t.getAttribute("aria-controls"));
      if (panel) panel.hidden = !on;
    });
    if (focus) tab.focus();
  }
  tabs.forEach(function (t, i) {
    t.addEventListener("click", function () { selectTab(t, false); });
    t.addEventListener("keydown", function (e) {
      var step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!step) return;
      e.preventDefault();
      selectTab(tabs[(i + step + tabs.length) % tabs.length], true);
    });
  });

  // The support button that follows the page: shown once past the top,
  // out of the way over the support section and the footer, and gone for a
  // week when its × is pressed.
  var pill = document.getElementById("support-pill");
  var hero = document.querySelector(".hero");
  var support = document.getElementById("support");
  var footer = document.querySelector(".footer");
  var WEEK = 7 * 24 * 3600 * 1000;
  function pillDismissed() {
    try { return Date.now() - Number(localStorage.getItem("bananav-pill-hidden") || 0) < WEEK; } catch (e) { return false; }
  }
  if (pill && hero && !pillDismissed()) {
    pill.hidden = false;
    pill.classList.add("away");
    var past = false, covered = false;
    var update = function () { pill.classList.toggle("away", !past || covered); };
    if ("IntersectionObserver" in window) {
      new IntersectionObserver(function (entries) {
        past = !entries[0].isIntersecting;
        update();
      }, { rootMargin: "-40% 0px 0px 0px" }).observe(hero);
      var showing = new Set();
      var watch = new IntersectionObserver(function (entries) {
        entries.forEach(function (en) { if (en.isIntersecting) showing.add(en.target); else showing.delete(en.target); });
        covered = showing.size > 0;
        update();
      });
      [support, footer].forEach(function (el) { if (el) watch.observe(el); });
    }
    pill.querySelector(".pill-close").addEventListener("click", function () {
      pill.hidden = true;
      try { localStorage.setItem("bananav-pill-hidden", String(Date.now())); } catch (e) {}
    });
  }

  // A download starting: it goes on as usual; a thank-you asks for support.
  var toast = document.getElementById("dl-toast");
  var toastTimer = null;
  function hideToast() {
    if (toast) toast.hidden = true;
    if (pill && !pill.hidden) pill.style.visibility = "";
  }
  if (toast) {
    document.querySelectorAll("[data-download]").forEach(function (a) {
      a.addEventListener("click", function () {
        toast.hidden = false;
        if (pill) pill.style.visibility = "hidden";
        clearTimeout(toastTimer);
        toastTimer = setTimeout(hideToast, 25000);
      });
    });
    toast.querySelector(".toast-close").addEventListener("click", hideToast);
    document.addEventListener("keydown", function (e) { if (e.key === "Escape") hideToast(); });
  }

  // Cards, highlights, downloads and posts fade in once.
  var items = document.querySelectorAll(".card, .hl, .dl, .post, .section-head");
  if ("IntersectionObserver" in window) {
    var seen = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          entry.target.classList.add("shown");
          seen.unobserve(entry.target);
        }
      });
    }, { rootMargin: "0px 0px -8% 0px" });
    items.forEach(function (el) { el.classList.add("reveal"); seen.observe(el); });
  }
})();
