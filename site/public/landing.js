(() => {
  const hero = document.querySelector(".hero");
  if (hero) {
    requestAnimationFrame(() => hero.classList.add("is-ready"));
  }

  const reveal = document.querySelectorAll(".band-inner, .capture-grid");
  if ("IntersectionObserver" in window) {
    const io = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-in");
            io.unobserve(entry.target);
          }
        }
      },
      { threshold: 0.18, rootMargin: "0px 0px -8% 0px" }
    );
    reveal.forEach((el) => io.observe(el));
  } else {
    reveal.forEach((el) => el.classList.add("is-in"));
  }

})();
