# Plan: solver 4x4x4 cu soluții mai scurte decât TPR

Actualizat: 2026-11-10. Țintă: medie cât mai aproape de 39 de mutări (OBTM).
Cifrele măsurate sunt în `bench/RESULTS.md`.

## Unde suntem

| Mod | Medie | Timp mediu | Măsurat pe |
|---|---|---|---|
| TPR (referință) | 44,57 | 0,15 s | 1.000 de cuburi |
| `--mode fast` | 41,54 | 0,36 s | primele 100 |
| implicit | 40,60 | 2,5 s | 1.000 de cuburi |
| `--mode full` | 40,00 | 15,9 s | primele 30 |

Alte mărimi: 2x2x2 se rezolvă optim în ~2 ms; 3x3x3 cu two-phase în ~10 ms sau
optim cu `--mode full`. Pornirea durează ~25 ms.

## De ce nu am schimbat structura fazei 3

Am verificat ideea prin numărarea stărilor, înainte să scriu cod. O fază cu
`S` stări și `b` continuări utile pe mutare cere cam `ln S / ln b` mutări.

| Etapă | Stări | Continuări utile | Estimat | Măsurat |
|---|---|---|---|---|
| Faza unită (toate mutările) | 1,5×10^14 | ~22 | 10,5 | 11,1 (primul capăt) |
| Faza 3 (17 mutări permise) | 2,8×10^13 | ~12 | 12,4 | 12,4 (TPR, un singur capăt) |
| 3x3x3 | 4,3×10^19 | ~13 | 17,5 | 17,7 (optim cunoscut) |

- Suma dă ~40,6, adică exact unde suntem. Structura în trei etape e la limita ei.
- Faza 3 pierde ~2,4 mutări fiindcă lucrează cu 12 continuări în loc de 22.
- Dacă îi dau fazei 3 mai multe mutări, coordonata ei de împerechere dispare și
  spațiul crește de la 2,4×10^8 la 3,2×10^11. Faza ar prelua muncă din faza
  liberă și totalul ar ieși mai lung, nu mai scurt.
- Singura cale de a recupera cele 2,4 mutări e ca reducerea să fie o singură
  fază liberă, iar pentru ea nu există un prag de căutare suficient de bun.
- Corectare (2026-11-11, măsurat cu `free3`): faza 3 nu pierde nimic din
  stările de la care pornește. Cea mai scurtă reducere cu toate mutările are
  aceeași lungime ca faza 3 pe toate cele 32 de cuburi cu faza 3 de cel mult
  12 mutări. Pierderea vine din obligația de a trece printr-o stare cu centrele
  așezate, nu din mutările restrânse ale fazei 3.

## Ce am încercat după 40,67

| Idee | Rezultat |
|---|---|
| Capete și reduceri fără mutare finală redundantă | corect logic, fără câștig măsurabil |
| Tabel 3x3x3 mai puternic (53 de miliarde de clase, 1,7 GB) | căutarea exactă de ~5 ori mai rapidă |
| Căutare exactă 3x3x3 peste lot, până la 16 mutări | −0,4 la ~10 s în plus (modul full) |

## Cât de departe e 39

Pe aceleași cuburi, lotul de reduceri conține rar un 3x3x3 de 15 mutări
(2,5 la mie din stări) și destul de des unul de 16 (3%). Căutarea exactă îl
găsește pe cel de 16, dar o întrebare „există soluție în 16?" costă ~2 s.

- **~40,2** e atins de modul full, în ~14 s.
- **~39,8** ar cere loturi de 10 ori mai mari sau o căutare exactă de 10 ori
  mai rapidă. Fiecare factor de 13 în viteză aduce cam o treime de mutare.
- **39** cere o idee nouă. Nu o am încă și nu vreau să promit.

Idei rămase, în ordinea în care le-aș încerca:

1. Prag de căutare 3x3x3 pe starea inversă (maximul din 6 tabele în loc de 3):
   de 2–3 ori mai puține noduri.
2. Pruning pe muchii în faza unită: un nivel în plus în același timp, ~−0,2.
3. Tabel de împerechere pentru 4–5 perechi sub toate mutările, ca faza liberă
   să poată cere „k perechi deja făcute": −0,5 estimat, muncă multă.

## Banc cu reduceri de lungime cunoscută (2026-11-10)

`anycube known --walk K`: un cub redus aleator primește K mutări aleatoare
din toate cele 36, deci există o reducere de cel mult K mutări. Cifrele sunt
în `bench/RESULTS.md`; pe scurt, 30 de cuburi pe lungime:

| K | Cea mai scurtă reducere găsită | Peste cea cunoscută |
|---|---|---|
| 14 | 18,00 | +4,0 |
| 16 | 20,90 | +4,9 |
| 18 | 22,43 | +4,4 |
| 20 | 22,73 | +2,7 |
| cuburi aleatoare | 23,10 | necunoscut |

- La K = 20 cubul e aproape aleator pentru solver (22,73 față de 23,10), iar
  reducerea în trei faze e cu 2,7 mutări mai lungă decât una care există
  sigur. Asta susține estimarea de ~20,5 pentru o reducere liberă.
- La K = 14, pe 12 cuburi din 30 solverul găsește reducerea cunoscută sau una
  mai scurtă, iar pe alte 12 ajunge la 21–24, ca pe un cub aleator:
  apropierea de un cub redus nu îl ajută decât dacă drumul scurt se
  potrivește cu fazele.
- Bancul e măsura pentru orice căutare liberă a reducerii: o idee nouă
  trebuie să găsească ≤ K aici înainte să fie încercată pe cuburi aleatoare.

## Căutare liberă a reducerii (2026-11-11)

Cod în `src/free.rs`, cifre în `bench/RESULTS.md`.

- **Clasa unui cub** (`key`): cuburile care diferă doar prin mutări de față și
  rotații făcute înainte au aceleași reduceri. Cheia e formată din centre,
  perechile de poziții care țin aceeași muchie și două parități.
- **Tabelul din jurul țintei** (`Ball`): toate clasele la cel mult 6 mutări de
  un cub redus, 24,8 milioane, generat în 5 s. Numărul lor crește de ~19,6 ori
  pe mutare, nu de 22. Cu asta, cea mai scurtă reducere liberă a unui cub
  aleator iese ~21 de mutări, nu 20,5; solverul găsește 23,1.
- **Faza 3 e deja optimă** pentru stările ei (vezi corectarea de mai sus).
- **Beam search cu toate mutările**, ghidat de distanța exactă de așezare a
  centrelor și de numărul de muchii împerecheate, lățime 300.000, ~10–17 s pe
  cub pe o mașină încărcată:

| K | Cuburi | Beam ≤ K | Solver ≤ K | Beam mai scurt decât solverul |
|---|---|---|---|---|
| 14 | 30 | 13 | 12 | 6 |
| 16 | 20 | 2 | 3 | 0 |
| 18 | 12 | 3 | 2 | 3 |
| 20 | 12 | 0 | 0 | 1 |
| aleatoare | 6 | | | 1 |

- La 14 mutări beam-ul găsește reducerea cunoscută pe alte cuburi decât
  solverul, deci se completează. De la 16 în sus nu mai găsește nimic pe
  majoritatea cuburilor, iar pe cuburi aleatoare e mai slab decât solverul.
- Cauza: semnalele se sting. La 14 mutări de un cub redus, distanța de
  așezare a centrelor e 7,8 față de 9,1 la un cub aleator, iar muchiile
  împerecheate 1,4 față de 0,5. O căutare liberă are nevoie de un semnal
  care să vadă la 15 mutări distanță; semnalele făcute de mână nu văd.
- Coada reducerii e deja optimă (`free-tail`, 24 de cuburi): tăiată cu 0–3
  mutări înainte de sfârșitul fazei libere și refăcută exact cu toate
  mutările, iese mai scurtă cu o mutare pe 2 cuburi din 23 și pe niciunul din
  cele 8 la care s-au putut tăia 3 mutări. Ce pierde reducerea în trei faze
  pierde în primele ~10 mutări, adică în direcția aleasă la început.
- Rezolvarea cubului invers NU merge la 4x4x4 (verificat: 0 din 60 de soluții
  întoarse rezolvă cubul real). Centrele de aceeași culoare nu se deosebesc,
  iar permutarea lor ascunsă iese, după întoarcere, ca centre pe fețe greșite.
  Cifrele 40,37 și 39,67 notate inițial aici nu erau rezultate reale.
- Rețea mică încercată (`ml/`, 4,4 milioane de parametri, 192 de milioane de
  cuburi, 36 de minute pe GPU-ul Mac-ului): deosebește de ~10 ori mai bine
  decât scorul de mână, dar beam search-ul ghidat de ea găsește reduceri de
  27–30 de mutări pe o minoritate de cuburi aleatoare, față de 23 la solver.
  La scara asta nu merge; o încercare serioasă ar însemna de zeci de ori mai
  multă antrenare și lățimi de căutare mult mai mari, fără garanție.
- O mutare W pusă înaintea amestecului NU merge nici ea, din același motiv
  (verificat: 0 din 3.240 de încercări rezolvă cubul real). Cifra de 39,85
  notată inițial aici nu era un rezultat real.
- Regulă de lucru de acum: o lungime contează doar dacă soluția a fost
  aplicată pe cubul inițial și l-a rezolvat.
- Cuburi puțin amestecate: 10 mutări → 10, 15 → 28, 20 → 42. Solverul fie
  găsește soluția scurtă, fie cade la 30–42; începe să rateze de la 12
  mutări (2 din 6 cuburi). Căutarea exactă cu tabel în jurul cubului rezolvat
  (`short`) dă optimul până la 12 mutări, dar e lentă și nu ajunge la 14–20.
- Ce rămâne: un prag învățat (rețea antrenată pe cuburi cu distanță
  cunoscută; bancul și tabelul dau etichete exacte până la 12 mutări). Câștigul
  maxim pe cuburi aleatoare e ~2 mutări, iar reușita nu e sigură.

## Viteză

Făcut pe 2026-11-10:

| Pas | Efect măsurat |
|---|---|
| Tabelele mici salvate pe disc | pornire din ~540 ms în ~25 ms |
| Faza 1 a 3x3x3 verificată cu tabelul exact | etapa 3x3x3 de ~5 ori mai rapidă |
| Ultima mutare a fazei unite testată întâi pe muchii | faza unită de 2–3 ori mai rapidă |
| Mutări care îndepărtează centrele, sărite când nu e loc de ele | inclus în cifra de mai sus |

Efectul pe ansamblu: aceeași lungime ca înainte (40,6) în 2,6 s în loc de 3,7 s,
iar modul full ajunge la ~40,0 fiindcă își permite de 15 ori mai multe capete.

Ce a mai rămas:

- Faza unită e în continuare ~60% din timp. Nodurile ei sunt răspândite pe
  toate nivelurile (nu doar la frunze), deci următorul câștig cere un test pe
  muchii cu două mutări înainte de capăt, nu doar cu una.
- Tabel de împerechere redus prin simetrie (240 MB → 30 MB), pentru faza 3.
- Mod fără tabelul de centre, pentru mașini cu puțină memorie.

## Moduri

- **fast** (~0,3 s): fază unită mică, fără adâncire 3x3x3. Există.
- **normal** (~4 s): implicitul de acum.
- **full** (~14 s): implicit plus căutare exactă 3x3x3. Există.
- De făcut: un mod **tiny**, fără tabelele mari, pentru mașini cu puțină memorie.

## 5x5x5: unde stau ceilalți și ce am pregătit

**Chen Shuang (cube555), citit din cod.** Cinci faze de reducere, apoi min2phase:

| Fază | Ce rezolvă | Mutări permise |
|---|---|---|
| 1 | centrele x și t de culoare R/L ajung pe fețele R/L | toate cele 36 |
| 2 | centrele U/D pe U/D, paritatea wings | 22 |
| 3 | R față de L pe fețele R/L, wings împărțite high/low, orientarea muchiilor centrale | 24 |
| 4 | patru muchii complete așezate, centre parțial | 16 |
| 5 | ultimele opt muchii și toate centrele | 14 |

- Rezultat publicat: 51–52 de mutări pentru reducere, 1–2 s, tabele de 240 MB.
  Cu 3x3x3 (~19,6) iese ~71 în total. dwalton76 raportează 78,9 doar pentru
  reducere, deci ~99 în total (corectat 2026-11-10: scrisesem „~77–79” ca
  și cum ar fi totalul).
- Unde pierde mutări, după cod:
  - etapa 3x3x3 se rezolvă o singură dată, după prima reducere găsită;
  - mutările nu se anulează la trecerea dintre faze;
  - fazele 1 și 2 sunt separate, ca la TPR înainte să le unim noi;
  - nu încearcă scramble-ul invers (corectat 2026-11-11: nu e o pierdere;
    inversul nu merge pe cuburi cu centre care nu se deosebesc între ele).
- Corectare (2026-11-10, după ce am citit `Search.java` și `PhaseSearch.java`):
  scrisesem aici că fazele lui taie lacom și că nu încearcă alte axe. Nu e
  așa. Fiecare fază pornește din 200–500 de soluții ale fazei dinainte,
  ordonate după lungimea totală, în 3 axe, iar faza 4 în două variante de
  felie. De aici vine o bună parte din rezultatul lui.

**Ce se transferă direct de la 4x4x4:**

- Lotul de reduceri comparat după lungimea totală (la 4x4x4 a adus ~2 mutări).
- Tabelul exact de centre: centrele x de la 5x5x5 se mișcă exact ca centrele
  de la 4x4x4, deci `centres-exact.bin` se refolosește ca atare. Centrele t au
  nevoie de un tabel propriu de aceeași mărime.
- Testul pe muchii la ultima mutare și căutarea pe trei axe deodată.

**Ce e diferit:**

- Centrele sunt fixe, deci cubul nu poate fi terminat în 24 de orientări; rămân
  doar cele 3 alegeri de axă.
- Două orbite de centre trebuie așezate împreună; pragul de căutare va fi
  maximul celor două tabele exacte, nu un singur tabel perfect.
- Muchiile centrale fixează orientarea, deci nu există paritatea de la 4x4x4,
  dar faza de muchii are trei piese pe muchie în loc de două.

**Făcut până acum (2026-11-10):**

- `src/cuben.rs`: model pe stickere pentru orice mărime, din geometrie.
- `src/c5.rs`: model pe piese. Un 5x5x5 fără rândurile din mijloc e un 4x4x4,
  deci modelul e cubul de 4x4x4 plus muchiile centrale plus centrele t.
- Set fix de 200 de scramble-uri și măsurătoarea cube555: 70,58 în total.
- `src/stage5.rs`: așezarea centrelor (ce fac fazele 1 și 2 la cube555).
  Faza A caută cu toate mutările, faza B are tabel exact (331 de milioane de
  stări). Rezultat: 17,1–17,6 mutări față de 18,28 la cube555.

**Moduri cerute, pentru toate mărimile:**

| Mod | Timp pe cub |
|---|---|
| fast | sub 1 s |
| normal | sub 5 s |
| full | fără limită |

Sunt implementate la 5x5x5 (`--mode`). Timpul se împarte pe faze: centre până
la 15% din buget, faza C până la 40%, P până la 70%, E până la 90%; o fază
întârziată se oprește doar după ce are destule soluții pentru următoarea.

**Solverul complet (2026-11-10), `src/edges5.rs` și `src/solver5.rs`:**

| Mod | Lungime medie | Timp mediu | Cuburi |
|---|---|---|---|
| cube555 | 70,58 | 1,1 s | 200 |
| fast | 70,16 | 0,34 s (max 0,74 s) | 200 |
| normal | 68,82 | 1,4 s (max 3,5 s) | 200 |
| full | 68,20 | 5,3 s | primele 40 |

- Prima încercare (faze C, D în doi pași, E; fiecare fază pornită din câteva
  soluții) a dat 73,0, mai slab decât cube555.
- Ce a adus rezultatul de acum:
  - faza P, ca faza 4 de la cube555, dar cu tabel exact pentru toate cele 12
    piese ale celor patru muchii (198 de milioane de stări) și tabele muchii
    contra centre; fără ele faza dura 5 s, cu ele 0,15 s;
  - fiecare fază pornește din toate soluțiile fazei dinainte și păstrează
    continuările cele mai scurte în total;
  - șase orientări pentru muchii (3 axe × 2 felii);
  - etapa 3x3x3 rezolvată pentru zeci sau sute de reduceri.
- Pe faze, modul normal față de cube555: centre 18,44 față de 18,28; muchii
  33,20 față de 33,05; 3x3x3 17,18 față de 19,23. Deci reducerea e la fel de
  lungă ca a lui; tot câștigul vine din etapa 3x3x3 aleasă din lot. Ținta de
  66 pe care o scrisesem aici nu e atinsă.
- Lățimea căutării ajută puțin: de 20 de ori mai lat aduce 0,6 mutări.

**Făcut după prima versiune completă (2026-11-10):**

- Tabelul fazei C redus prin cele 16 simetrii ale fazei: 2,77 GB → 175 MB,
  generat în 17 s. Simetriile se găsesc din tabelele de mutări, nu din
  geometrie (`src/sym.rs`). Verificat pe 20 de milioane de intrări față de
  tabelul vechi. Tabelele de 5x5x5 au acum 2,1 GB în loc de 4,7 GB, iar
  timpii au scăzut cu ~35% pe un Mac ocupat.
- Centrele într-o singură fază (`src/centres5.rs`): tabele exacte pe fiecare
  orbită (9,46 miliarde de colorări, 300 MB după simetrie; o orbită singură
  cere cel mult 11–12 mutări, în medie ~9). Căutarea exactă cu pragul
  „maximul celor două” a găsit 15 mutări în loc de 17 la cubul măsurat, dar
  în 340 s pe 8 nuclee: pragul e ~9, răspunsul ~15. Tăierea după suma celor
  două distanțe a pierdut soluțiile scurte (a găsit 17–19 în 20 s). Nu e
  folosită în niciun mod; se pornește cu `--joint`.

**Pași următori pentru 5x5x5:**

1. Centre: un prag mai bun decât maximul pe orbite. Candidat: un tabel comun
   pentru centrele unei singure axe din ambele orbite (problema fazei A),
   care ar avea 34 de miliarde de intrări după simetrie, deci doar o parte
   din el (stările apropiate de țintă) încape. Câștig posibil: până la 2
   mutări, dacă soluțiile de 15–16 rămân bune și pentru fazele de muchii.
2. Fazele P și E: soluțiile bune se găsesc la 2–4 mutări peste prag, deci
   tabele comune muchii+centre mai mari ar grăbi căutarea și ar permite un
   lot mai lat în același timp.
3. Unirea fazelor E și 3x3x3 pentru ultimele mutări, ca la 4x4x4.

## 6x6x6 și 7x7x7 (început 2026-11-10)

Ordinea stabilită cu Horia: 6 și 7 optimizate ca mutări și ca timp, apoi
NxNxN general ca baseline, apoi îmbunătățiri pe rând.

**Referința.** dwalton76, rulat pe seturile noastre din instalarea lui Horia
(`~/MultiCuber/rubiks-cube-NxNxN-solver`, cu tabelele deja descărcate).
Tabelul din README-ul lui spune „mutări până la 3x3x3”, dar cifrele sunt
totaluri: la 4x4x4 am măsurat 51,71 pe 100 de cuburi (README: 51,6), toate
soluțiile verificate. Rezultatele pe 5, 6 și 7 sunt în `bench/RESULTS.md`.

**Ce e nou la 6x6x6.** Patru orbite de centre (x interior, x exterior, două
orbite oblice în oglindă) și două orbite de wings, fără piese fixe. Tot ce
avem se refolosește prin „cuburi virtuale” (`Geometry::sub_facelets`):

- straturile 1, 3, 4, 6 formează un 4x4x4 (colțuri, wings interioare, x
  interior); mutarea lui lată e mutarea de 3 straturi;
- straturile 1, 2, 3|4, 5, 6 formează un 5x5x5 dacă cele două straturi din
  mijloc arată la fel; mutările lui sunt chiar mutările de 1 și 2 straturi.

Observația pe care se sprijină planul: sub mutările 4x4x4-ului virtual,
fiecare sfert de față (un x interior, cei doi oblici de lângă el și x-ul
exterior din colț) se mișcă rigid. Dacă fiecare sfert are o singură culoare,
toate centrele se comportă ca centrele unui 4x4x4.

**Planul pentru prima versiune:**

1. Centrele celor patru orbite pe axele lor, în patru pași: R/L pentru x
   interior și oblici (toate cele 54 de mutări), apoi R/L pentru x exterior
   cu faza A de la 5x5x5 pe cubul virtual (perechile de oblici sunt centrele
   lui t); la fel pentru U/D, cu tabelul exact al fazei B.
2. Sferturile într-o singură culoare: doar jumătăți de tură late și fețe;
   pe fiecare axă starea are 70⁴ valori, deci tabel exact pe axă.
3. Solverul nostru de 4x4x4 pe cubul virtual, fără etapa 3x3x3: rezolvă
   toate centrele și împerechează wings interioare, cu paritate cu tot.
4. Solverul nostru de 5x5x5 pe cubul virtual: centrele sunt gata, deci
   rămân fazele de muchii și 3x3x3.

Estimare: ~37 + ~13 + ~23 + ~50 ≈ 120–125 de mutări. E o estimare, nu o
măsurătoare.

**Stadiu (2026-11-10):** pasul 1 e scris și verificat (`src/stage6.rs`):
36,55 de mutări în 0,48 s, față de ~38 la dwalton76. A ieșit în patru
căutări, nu cum scrie mai sus: (a) x interior R/L pe fețe și oblicii R/L
împerecheați, toate mutările; (b) faza A de la 5x5x5 pe cubul virtual;
(c) x interior U/D, paritatea wings interioare și oblicii U/D împerecheați;
(d) faza B de la 5x5x5. Împerecherea în loc de așezare a adus 14 mutări.
Pașii 2–4 urmează.

**Solver complet (2026-11-10, seara):** 113,2 mutări pe primele 6 cuburi
(dwalton76: 136,1), adică −17%; ținta lui Horia e −20% (108,9). Timpul e
4–46 s, cu un cub la 4 minute. Planul de mai sus s-a schimbat pe drum:

- „Sferturile într-o culoare” costau 11,5 mutări doar ca să poată lucra
  4x4x4-ul virtual. Le-am înlocuit cu o reducere interioară proprie
  (`src/solver6.rs`): mutările ultimei faze de la 4x4x4, cu tabelele ei de
  împerechere ca prag, care face și perechile de oblici de o singură
  culoare. Exact, pasul are ~16 mutări în loc de 11,5 + 12.
- Problema rămasă e căutarea: pragul spune ~10, soluția are 16–19, fiindcă
  împerecherea wings interioare și cea a oblicilor aproape nu împart
  mutări. Căutarea exactă durează de la secunde la zeci de minute.

**Stadiu 2026-11-11:** trei moduri. Rapid: 119,0 mutări în 2,8 s; normal:
116,95 în 5,6 s (20 de cuburi); exact (modul full): ~112, de la 40 s la
minute. dwalton76: 136,1 în 43 s. Ținta de −20% (108,9) nu e atinsă: rapid
−12,6%, normal −14,1%, exact −17%.

De ce nu ajunge schema de acum, după numărătoarea de stări:

- așezarea a patru orbite de centre pe axe costă 36 de mutări; fără ea,
  solverul de 5x5x5 și-ar așeza singur centrele în 18,4;
- ce trebuie de fapt făcut înainte de 5x5x5 e doar: x interior rezolvat,
  wings interioare împerecheate, perechile de oblici de o culoare. După
  numărul de stări asta ar cere 26–32 de mutări, deci un total de ~95–101;
- piedica e aceeași în orice variantă: o căutare care împerechează wings
  interioare și oblicii împreună, cu un prag bun. Fără prag, e lentă.

**Stadiu 2026-11-11, mai târziu:** două schimbări au adus ~7 mutări:
pașii 3 și 4 ai așezării într-o singură căutare în fascicul (17,25 → 15,75
mutări) și reducerea interioară cu oblici făcută cu fasciculul scris pentru
7x7x7 (17 mutări într-o fracțiune de secundă, unde căutarea exactă lua
minute). Calea scurtă (potrivirea oblicilor cu x interior) a rămas doar ca
rezervă. Cifrele pe moduri sunt în `bench/RESULTS.md` și în README.

Ce nu a mers după aceea (detalii în `bench/RESULTS.md`): împărțirea wings
interioare făcută în pasul 3 al așezării (aceeași sumă), mai multe
rezultate duse din pasul 1 în pasul 2, alegerea așezării după împărțirea
care urmează, un singur fascicul pentru toate variantele unui cub, pașii 1
și 2 (axa R/L) într-un singur fascicul.

Ce a mai adus puțin: capătul dinspre 3x3x3. Solverul de 5x5x5 ajunge aici
la ~260 de reduceri (din 4 dintre cuburile primite), față de ~1000 pe un
5x5x5 propriu, iar 3x3x3-ul rămas are 18 mutări în loc de 16,5. Cu mai
multe reduceri păstrate și mai multe căutări de 3x3x3 se câștigă ~0,8.

**Cifre la sfârșitul zilei de 2026-11-11** (20 de cuburi, toate soluțiile
verificate; dwalton76 136,10): rapid 112,0 în 4,1 s (−17,7%), normal 109,2
în 9,7 s (−19,8%), full 108,85 sub un minut (−20,0%; 108,6 pe primele 10
cuburi, cele pe care a fost măsurat dwalton76). Ținta de 108,9 e atinsă la
limită doar în modul full.

**Ce urmează la 6x6x6, în ordine:**

1. Axa R/L a așezării (pașii 1 și 2, 18,85 mutări): singura parte rămasă în
   doi pași cu căutare exactă pe fiecare. Un fascicul pe suma distanțelor
   pe orbite nu merge; ar trebui un scor care vede două orbite deodată.
2. Diversitatea reducerilor date 3x3x3-ului (vezi mai sus).
3. Timpul: modul rapid e la ~3 s, nu sub 1 s.

**7x7x7** urmează aceeași schemă: 5x5x5 virtual din straturile 1, 3, 4, 5, 7
pentru interior, tripletele oblic–t–oblic în locul perechilor, apoi 5x5x5
virtual din straturile 1, 2, 4, 6, 7.

**7x7x7, stadiu 2026-11-11:** solver complet (`src/stage7.rs`,
`src/solver7.rs`, `src/search.rs`), scris de un agent pe ramura
`seven-by-seven` și adus în `shorter-solutions`. Ținta e atinsă la limită:
154,56 de mutări pe primele 50 de cuburi (dwalton76: 194,40 pe primele 10;
ținta 155,5), ~6 s pe cub; modul rapid 155,6 în 3,3 s. Am rulat separat 20
de cuburi pe care agentul nu le folosise: 155,00, toate verificate.
Împărțirea: așezare 53, potrivire 16, muchii interioare 34, muchii
exterioare 35, 3x3x3 17. Ce a mers acolo și nu la 6x6x6: căutarea în
fascicul (beam) cu 100.000 de stări pe nivel, acolo unde pragul e mult sub
răspuns. Punctele slabe: așezarea (53) și potrivirea (16 mutări de pură
pregătire). Timpul e peste țintele de 1 s și 5 s.

**Kociemba, RubikNxNxNSolver** (citit 2026-11-10): rezolvă doar centrele,
în metrica de felii simple, ~1,19 mutări pe centru la 101x101x101; pentru
7x7x7 ar însemna ~170 de mutări doar pe centre, deci nu ajută la 6 și 7.
Pentru solverul general e scheletul potrivit: fiecare pereche de orbite
oblice (x, y) se rezolvă împreună, doar cu fețe și feliile x și y, iar
orbita „+” a fiecărei felii e martorul că felia a fost întoarsă la loc, deci
perechile deja rezolvate nu se strică.

## Orice mărime (2026-11-11)

Direcția dată de Horia: nu trebuie să fim perfecți acum, țintele erau
relative; întâi o fundație curată, cu NxNxN general testat până la
10x10x10 față de dwalton76, apoi viteză și eficiență.

**Ce există:** `src/nnn.rs` rezolvă orice cub de la 6x6x6 în sus cu același
cod (testat pe 6–12 și 14). Ideea: câteva straturi alese simetric, cu
straturile dintre ele mișcându-se odată cu stratul ales de lângă ele
dinspre mijloc, se poartă ca un cub mic; ce e rezolvat într-un astfel de
bloc rămâne rezolvat când se rezolvă cubul mic.

- Centrele: inel cu inel din mijloc spre exterior, câte două orbite oblice
  în oglindă, fiecare pas un 6x6x6 sau 7x7x7 virtual. Trei treceri peste
  toți pașii: R/L, apoi celelalte axe (cu așezările de la 6x6x6 și 7x7x7),
  apoi „acasă” cu fețe și jumătăți de tură adânci (`src/axes.rs`).
- Muchiile: orbită cu orbită din mijloc spre exterior, fiecare un 5x5x5
  virtual (un 4x4x4 pentru wings din mijloc la cuburile pare); ultima e
  tot cubul ca 5x5x5, dus până la capăt.

**Cifre** (20 de cuburi; dwalton76 pe primele 5): 8x8x8 301,9 (308,4),
9x9x9 368,55 (368,0), 10x10x10 531,0 (523,4); de ~3 ori mai rapid ca el.

**Trebuie optimizat pe mărimi?** Nu pe mărimi, ci pe pași: tot ce e peste 7
trece prin aceiași pași virtuali de 4–7 straturi, deci orice îmbunătățire
a unui pas se vede la toate mărimile. Dar solverul general e mult sub cele
dedicate: pe 6x6x6 dă 146 (dedicat: 109), pe 7x7x7 190 (dedicat: 155).

**A doua versiune (2026-11-11, seara):** ideea de la 6x6x6 și 7x7x7 dusă în
solverul general: centrele nu mai sunt aduse acasă, ci fiecare e făcut de
aceeași culoare cu cel de sub el (`src/rings.rs`), rând cu rând de la
margine spre interior, iar pașii de muchii le rezolvă odată cu centrele
din interior. 8x8x8: 264,5 (era 301,9; dwalton76 308,4). 9x9x9: 349,45
(368,55; 368,0). 10x10x10: 529,4, neschimbat, fiindcă pentru rândul de 6
centre nu găsesc potrivire și cubul merge pe drumul vechi.

**Cât se mai poate.** Măsura e raportul față de limita dată de numărul de
stări: solverele dedicate sunt la 1,38 (4), 1,44 (5), 1,63 (6), 1,67 (7);
cel general e la 2,26 (8), 2,34 (9), 2,96 (10). La raportul de la 6 și 7
ar ieși ~193, ~246 și ~295 de mutări. Deci impresia lui Horia e corectă:
de la 8 în sus se mai poate câștiga masiv.

**A treia versiune (2026-11-11, noaptea):** pașii de așezare doar formează
rândurile de centre; inelul întreg merge pe fețele lui o singură dată (ca
centrele x și t ale unui 5x5x5). 8x8x8: 246,5 (−20% față de dwalton76),
9x9x9: 333,8 (−9%), 10x10x10: 482,75 (−8%).

**De ce cuburile mari sunt mai departe de limită decât 4–7** (întrebarea
lui Horia): un solver de 4–7 își tratează toate piesele într-un plan cu
puține etape mari; cel general taie cubul în bucăți de câte două orbite
și fiecare bucată plătește singură, iar numărul de bucăți crește cu
pătratul mărimii. Nu e nevoie de reglaj pe mărimi, ci de etape generale
care lucrează pe rânduri sau inele întregi. Regula pe piese: solverele de
4–7 fac 0,71 mutări pe piesă (6n² − 12n + 8 piese); la rata asta ar ieși
~210 la 8, ~274 la 9, ~346 la 10. Regula lui Horia (creșterea se mărește
cu 10 la fiecare mărime) dă 219 / 289 / 369 și se potrivește la fel de
bine pe 3–7.

**A patra versiune (2026-11-11, după-amiază):** potrivirea rândurilor de
orice lungime. De la mijlocul rândului spre capete, cu coloanele deja
făcute ținute identice între ele; atunci, oricâte ar fi, contează ca una
singură și fiecare pas e o căutare mică, de mărime fixă. 9x9x9: 326,9
(−11% față de dwalton76), 10x10x10: 427,95 (−18%); niciun cub nu mai ia
drumul lung.

**Ce urmează, pe idei proprii (nu după dwalton76 sau Kociemba):**

1. Timpul: nemăsurat cinstit (rulau alte trei sesiuni), dar potrivirea
   încearcă multe variante de semn și fascicule late; de strâns.
2. Așezarea: 165 de mutări la 10x10x10, cea mai mare parte.
3. Muchiile: ~35 de mutări pe orbită.
4. Tabelele potrivirii: 24 MB fiecare, multe sunt probabil aceleași între
   mărimi și axe.

## Punerea în comun (2026-11-11, după-amiază)

Patru sesiuni au lucrat în paralel, fiecare pe ramura ei; la cererea lui
Horia au fost unite pe ramura `together` (unirea a făcut-o sesiunea de
6x6x6, cu acordul celor de 5x5x5 și 7x7x7; sesiunea de 4x4x4 nu a putut fi
anunțată, ramura ei a intrat cum era comisă). Detaliile fiecăreia sunt în
`bench/RESULTS.md`, în secțiunile zilei.

| Ramură | Ce aduce | Cifra ei, măsurată separat |
|---|---|---|
| `shorter-solutions` | potrivirea rândurilor de centre de orice lungime | 9x9x9 326,9; 10x10x10 427,95 |
| `faster-555` | faza A cu tabel aproape de țintă la 7–8 mutări și lungimi peste optim; fazele de muchii în ordinea rezervei; tabel exact pentru faza a doua a 3x3x3 | 5x5x5 implicit 68,83 → 67,75 cu o treime din timpul de procesor (12 cuburi, alăturat) |
| `faster-777` | un singur fascicul pentru toți candidații (`beam_roots`), pași de așezare uniți, ordinea axelor aleasă în căutare; așezarea cuburilor mari cu candidați (`staging.rs`) | 7x7x7 153,15 (20 de cuburi) la jumătate din timpul de procesor |
| `six-faster` | candidați fără dubluri, fascicule care nu mai așteaptă după memorie, ultimul pas al așezării o dată pe tipar; inelul interior al cuburilor pare prin pașii de 6x6x6 | 6x6x6 implicit 109,35 → 108,35, rapid 111,65 → 110,60 |
| `known-reductions` | experimente 4x4x4: reduceri cu toate mutările, rețea ca estimare, cuburi aproape reduse | nimic nou în solver; vezi mai jos ce au arătat |
| `public-cleanup` | README pentru publicare, licență MIT | |

**Ce au găsit sesiunile și e valabil pentru toate mărimile:**

1. **Căutările așteptau după memorie.** Tabelele sunt mult mai mari decât
   memoria cache, iar o stare citea câte una pe rând. Trei sesiuni au
   ajuns independent la același remediu: copiii unei stări se fac întâi,
   intrările lor se cer dinainte (`search::fetch`), abia apoi se citesc.
   Fascicule de 1,4–2,7 ori mai rapide, aceleași soluții.
2. **Candidați care sunt același cub.** La 6x6x6 o treime până la jumătate
   din candidații duși din pas în pas erau copii (același cub ajuns prin
   alte mutări); la startele 5x5x5 sunt 4–12%; la 7x7x7 așezarea elimina
   deja copiile, dar finalele nu. Eliminate după starea cubului, aceeași
   lățime dă mai multe cuburi diferite: e singurul lucru care a scurtat
   6x6x6 fără timp în plus.
3. **Diversitatea devreme bate lățimea.** 7x7x7: 8 așezări care diferă
   devreme dau o potrivire de 15,5, 256 care diferă doar la ultimele
   mutări 16,9. 5x5x5: lungimile fazei A peste optim valorează 1,85 +
   0,65 + 0,3 mutări, lățimile fazelor P și E nimic. 6x6x6: fascicule de
   trei ori mai late aduc 0,3.
4. **Mai multe starturi într-un singur fascicul**, nu un fascicul pentru
   fiecare (7x7x7): jumătate din cost la aceeași lungime, iar lățimea se
   duce la cele care ies bine. Alegerea axei ajută doar așa, ca rădăcini
   ale aceluiași fascicul; aleasă după lungimea unui pas nu aduce nimic.
   La 6x6x6 cele șase ținute ale cubului ca fascicule în plus nu au adus
   nimic la efort egal: de încercat ca rădăcini.
5. **Paritatea aripilor** lăsată liberă în așezare o scurtează cu 1–2
   mutări pe pas și lungește muchiile cu 11 la 10x10x10; o repară doar
   pasul inelului exterior.
6. **Un fascicul poate rămâne pe un platou** de stări care par aproape
   gata și nu sunt (reducerea interioară de la 6x6x6: cubul interior la o
   jumătate de tură adâncă de gata, care ar strica perechile de oblici).
   Regula de renunțare trebuie să fie a problemei: pusă la toate
   fasciculele a tăiat potrivirea rândurilor lungi.
7. **Pe mașina ocupată timpul de ceas nu spune nimic.** Au rulat cinci-șase
   benchmarkuri deodată (încărcare 60–340 pe 10 nuclee). Măsuri care nu
   depind de asta: timp de procesor (`src/clock.rs`), poziții căutate pe
   fază (`bench5`), lungimi fără ceas.

**Ce au arătat experimentele de 4x4x4** (nu schimbă solverul, dar
închid întrebări): cubul invers și „o mutare lată în față” NU dau soluții
(0 din 60 și 0 din 3.240 verificate; centrele unei culori nu se pot
deosebi, deci permutarea lor ascunsă iese ca centre greșite), iar cifrele
39,67 și 39,85 scrise mai devreme pentru ele nu sunt rezultate. Sfârșitul
reducerii (ultimele 12–13 mutări) e deja cât cel mai scurt posibil cu
orice mutare; ce pierde reducerea în trei faze, pierde în primele ~10
mutări. O rețea mică e de zece ori mai selectivă decât scorul de mână ca
estimare a distanței până la un cub redus, dar un fascicul ghidat de ea
găsește 27–30 de mutări pe o minoritate de cuburi, față de 23 ale
solverului.

**Ce nu a mers, ca să nu fie încercat din nou:** tabele cu starea
întreagă aproape de țintă pentru fazele P și E (pozițiile căutate nu
scad); mai multe soluții pe start în faza E; fascicule înguste pentru
mulți candidați în ultimul pas al așezării de 6x6x6; pașii 1–3 ai
așezării de 7x7x7 într-o singură căutare; ponderi diferite în scorurile
unite; fascicule mai late la așezare (100.000 → 300.000: 0,3 mutări).

**Ce a rămas de făcut după unire:**

1. Măsurătorile finale, pe rând, pe codul unit: fiecare mărime cu timp de
   ceas pe mașina liberă. Cifrele din tabelul de mai sus sunt ale ramurilor
   luate separat; README-ul are încă cifrele de dimineață.
2. Lățimile 5x5x5 scrise explicit în solverele de 6x6x6 și 7x7x7: fazele
   de muchii întorc acum cam câte rezultate li se cer, nu până la de opt
   ori mai multe, deci aceleași numere înseamnă mai puține reduceri.
3. În `staging.rs` pașii de tip 6x6x6 rulează încă o dată pe candidat;
   `stage6::stage_wide` caută deja ultimul pas o singură dată pe tipar și
   ar putea primi mai multe cuburi.
4. Ideea ramurii 5x5x5 „ultimele rânduri ale unui inel alese împreună cu
   pasul următor” a ieșit odată cu bucla veche de așezare din `nnn.rs` și
   nu e reaplicată în `staging.rs`.
5. `Stage5::a_close` (pragul fazei A folosit de fasciculele de 7x7x7)
   trece acum prin cele 16 simetrii la fiecare stare: de măsurat dacă
   încetinește așezarea de 7x7x7.

## Despre tabelele lui dwalton76

Pentru 4x4x4 are opt tabele cu între 840 și 5,9 milioane de rânduri, ca text:
fiecare rând e o stare și secvența care o rezolvă. Sunt mari pe disc din cauza
formatului, nu a numărului de stări. Gigabiții sunt mai ales pentru 5x5x5–7x7x7.
Ale noastre țin 2 biți pe stare (distanța modulo 3) și acoperă spații de până
la 9,5 miliarde de stări; de aici vine diferența de lungime, nu din mărime.

## Riscuri

- Tabelele ocupă 4,9 GB pe disc și se generează în ~4 minute la prima rulare.
- Limita de timp e „moale": fiecare etapă își așteaptă primul rezultat.
- Rezultatele variază cu câteva sutimi între rulări, din cauza paralelismului.

## Surse

- TPR: https://github.com/cs0x7f/TPR-4x4x4-Solver
- dwalton76: https://github.com/dwalton76/rubiks-cube-NxNxN-solver
- Limita 54 pentru fazele 1+2+3+3x3x3 (neverificată): https://github.com/AnanasClassic/4x4x4-obtm-54-research
