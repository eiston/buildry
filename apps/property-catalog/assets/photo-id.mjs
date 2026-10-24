const type = document.getElementById('document-type');
const back = document.getElementById('id-back');
const backLabel = document.getElementById('back-label');
const frontLabel = document.getElementById('front-label');

if (type && back && backLabel && frontLabel) {
  const update = () => {
    const passport = type.value === 'passport';
    back.required = !passport;
    back.hidden = passport;
    backLabel.hidden = passport;
    frontLabel.textContent = passport ? 'Photo and information page' : 'Front of ID';
    if (passport) back.value = '';
  };
  type.addEventListener('change', update);
  update();
}
