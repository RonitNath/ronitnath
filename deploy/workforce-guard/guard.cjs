'use strict';
const issuer='https://auth.isoastra.com';
const subjects=Object.freeze({'388597630173413379':'ronit@isoastra.com','388644017279107075':'support@isoastra.com'});
function allowed(subject,email) {
  return process.env.OIDC_ISSUER===issuer && typeof subject==='string' && typeof email==='string'
    && Object.prototype.hasOwnProperty.call(subjects,subject) && subjects[subject]===email.trim().toLowerCase();
}
function profile(profile) {return profile?.email_verified===true && allowed(profile.sub,profile.email);}
function principal(accounts,user) {
  // Preserve unrelated local-account permission; federated authority is exact sub.
  return accounts.length===0 || process.env.OIDC_ISSUER===issuer && accounts.every(account=>typeof account.subject==='string' && Object.prototype.hasOwnProperty.call(subjects,account.subject));
}
// Frozen native BetterCall recognizes APIError by name and serializes statusCode/body.
function denied() {
  const error=new Error('Access unavailable');error.name='APIError';error.status='FORBIDDEN';error.statusCode=403;
  error.body={message:'Access unavailable',code:'WORKFORCE_IDENTITY_DENIED'};error.headers={};return error;
}
module.exports={allowed,profile,principal,denied};
