const type = document.getElementById('document-type');
const back = document.getElementById('id-back');
const backLabel = document.getElementById('back-label');
const frontLabel = document.getElementById('front-label');
const backUpload = document.getElementById('back-upload');

if (type && back && backLabel && frontLabel) {
  const update = () => {
    const passport = type.value === 'passport';
    back.required = !passport;
    back.hidden = passport;
    backLabel.hidden = passport;
    if (backUpload) backUpload.hidden = passport;
    back.disabled = passport;
    frontLabel.textContent = passport ? 'Photo and information page' : 'Front of ID';
    if (passport) { back.value = ''; back.dispatchEvent(new Event('change', { bubbles: true })); }
  };
  type.addEventListener('change', update);
  update();
}
